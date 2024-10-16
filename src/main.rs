mod pty;

use clap::Parser;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ecs::Client as EcsClient;
use aws_sdk_ecs::Error as EcsError;
use aws_sdk_ecs::config::Region as Region;
use aws_sdk_sts::Client as StsClient;
use aws_sdk_sts::operation::get_caller_identity::GetCallerIdentityError;
use std::error::Error;
use std::process::{Command};
use aws_sdk_ecs::error::SdkError;
use aws_sdk_ecs::operation::describe_services::DescribeServicesError;
use inquire::{Select, Confirm, InquireError};
use inquire::error::InquireResult;
use futures::future::try_join_all;

/// A CLI tool to interactively run ECS `execute-command`
#[derive(Parser, Debug)]
#[command(author = "Kyrylo Kulyhin", version = "0.2.0", about = "ECS execute-command CLI tool", long_about = None
)]
struct Cli {
    #[arg(long, short = 'i', conflicts_with = "service")]
    interactive: bool,

    /// The AWS profile to use
    #[arg(long, short = 'p', default_value = "dt-infra")]
    profile: String,

    // The AWS region to use
    #[arg(long, short = 'r', default_value = "eu-north-1")]
    region: String,

    /// The ECS service name
    #[arg(required_unless_present = "interactive")]
    service: Option<String>,

    /// The container name in the ECS task
    #[arg(default_value = "app")]
    container: String,

    /// The command to run inside the container
    #[arg(default_value = "bash")]
    command: String,
}

#[macro_use]
extern crate ini;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    let mut aws_region = args.region.clone();
    let mut aws_profile = args.profile.clone();
    let container_name = args.container.clone();
    let interactive = args.interactive;

    if interactive {
        println!("Interactive mode is enabled.");

        let aws_config = std::env::var("HOME").unwrap() + "/.aws/config";
        let map = ini!(&*aws_config);

        let profiles = map.iter().filter(|(sec, _)| sec.starts_with("profile "));

        let mut options: Vec<String> = vec![];
        for profile in profiles {
            let replaced_profile = profile.0.replace("profile ", "");
            options.push(replaced_profile);
        }

        let ans: Result<String, InquireError> = Select::new("What profile to use?", options).prompt();
        match ans {
            Ok(ref choice) => println!("{}! is your choice!", choice),
            Err(_) => println!("There was an error, please try again"),
        }

        aws_profile = ans.unwrap();

        let mut region_options: Vec<String> = vec![];
        region_options.push("eu-north-1".to_string());
        region_options.push("eu-central-1".to_string());
        region_options.push("eu-west-2".to_string());

        let region_ans: Result<String, InquireError> = Select::new("What region to use?", region_options).prompt();
        match region_ans {
            Ok(ref choice) => println!("{}! is your choice!", choice),
            Err(_) => println!("There was an error, please try again"),
        }

        aws_region = region_ans.unwrap();
    }

    match check_sso_session(&aws_profile).await {
        Ok(_) => {
            println!("SSO session is active. Proceeding with ECS operations for container: {}", container_name);
        }
        Err(_) => {
            match prompt_user_for_login() {
                Ok(true) => {
                    let status = Command::new("aws")
                        .arg("--profile")
                        .arg(aws_profile.clone())
                        .arg("sso")
                        .arg("login")
                        .status()?; // Wait for the command to complete

                    if !status.success() {
                        println!("AWS SSO login failed. Exiting the program.");
                        return Ok(()); // Exit if login fails
                    }
                    println!("Please run `aws sso login` in another terminal, then re-run this program.");
                },
                Ok(false) => {
                    println!("User chose not to log in.");
                    println!("Exiting the program.");
                },
                Err(e) => {
                    eprintln!("Error: {}", e);
                    // Handle the error appropriately
                }
            }
        }
    }

    let region_provider = RegionProviderChain::first_try(Region::new(aws_region.clone())).or_default_provider();

    let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region_provider)
        .profile_name(aws_profile.clone())
        .load()
        .await;

    let ecs_client = EcsClient::new(&shared_config);
    let cluster_arn = show_clusters(&ecs_client).await?;
    let service;
    if interactive {
        let services = list_services(&ecs_client, &cluster_arn).await?;

        // let service_options = services.iter().map(|service| service.service_name().unwrap().to_string()).collect::<Vec<String>>();
        let service_ans: Result<String, InquireError> = Select::new("What service to use?", services).prompt();
        match service_ans {
            Ok(ref choice) => println!("{}! is your choice!", choice),
            Err(_) => println!("There was an error, please try again"),
        }
        service = service_ans.unwrap();
    } else {
        service = args.service.as_deref().unwrap_or_else(|| {
            eprintln!("Error: Service must be provided in non-interactive mode.");
            std::process::exit(1);
        }).to_string();
    }
    let task_arn = show_tasks(&ecs_client, &cluster_arn, &service).await?;

    let cmd = "aws";
    let args = [
        "--profile", &*aws_profile,
        "--region", &*aws_region,
        "ecs", "execute-command",
        "--cluster", &*cluster_arn,
        "--task", &*task_arn,
        "--container", &*container_name,
        "--interactive",
        "--command", &*args.command,
    ];

    pty::spawn_pty_shell(cmd, &args)?;

    Ok(())
}

async fn check_sso_session(profile: &str) -> Result<(), aws_sdk_sts::error::SdkError<GetCallerIdentityError>> {
    let region_provider = RegionProviderChain::default_provider();
    let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region_provider)
        .profile_name(profile)
        .load()
        .await;

    let sts_client = StsClient::new(&shared_config);

    // Call `get_caller_identity` to verify the session is active
    match sts_client.get_caller_identity().send().await {
        Ok(_) => Ok(()),
        Err(err) => match err {
            aws_sdk_sts::error::SdkError::ServiceError { .. } => {
                println!("Service error occurred: {:?}", err);
                Err(err)
            }

            aws_sdk_sts::error::SdkError::TimeoutError(_) => {
                println!("The request timed out. Please check your connection.");
                Err(err)
            }
            aws_sdk_sts::error::SdkError::DispatchFailure(_) => {
                println!("Network error. Please check your internet connection.");
                Err(err)
            }
            _ => {
                println!("An unknown error occurred: {:?}", err);
                Err(err)
            }
        },
    }
}

fn prompt_user_for_login() -> InquireResult<bool> {
    println!("No active AWS SSO session found.");
    println!("Please run the following command to login via AWS SSO:");
    println!("  aws sso login");

    let ans = Confirm::new("Would you like to retry after logging in? (Y/n): ")
        .with_default(true)
        .with_help_message("You'll be automatically redirected to the AWS SSO login page.")
        .prompt();

    match ans {
        Ok(true) => println!("That's awesome!"),
        Ok(false) => println!("That's too bad, I've heard great things about it."),
        Err(_) => println!("Error with questionnaire, try again later"),
    }

    ans
    // print!("Would you like to retry after logging in? (Y/n): ");
    // io::stdout().flush().unwrap(); // Make sure prompt gets printed immediately
    //
    // let mut input = String::new();
    // io::stdin().read_line(&mut input).expect("Failed to read input");
    // let input = input.trim().to_lowercase();
    //
    // input == "y" || input == "yes"
}

// List your clusters.
async fn show_clusters(client: &aws_sdk_ecs::Client) -> Result<String, EcsError> {
    let resp = client.list_clusters().send().await?;

    let cluster_arns = resp.cluster_arns();
    println!("Found {} clusters:", cluster_arns.len());

    let clusters = client
        .describe_clusters()
        .set_clusters(Some(cluster_arns.into()))
        .send()
        .await?;

    for cluster in clusters.clusters() {
        if let Some(cluster_name) = cluster.cluster_name() {
            if cluster_name.ends_with("-main") {
                if let Some(cluster_arn) = cluster.cluster_arn() {
                    println!("  ARN:  {}", cluster_arn);
                    println!("  Name: {}", cluster_name);
                    return Ok(cluster_arn.to_string());
                }
            }
        }
    }

    Ok("".to_string())
}

// List your tasks.
async fn show_tasks(client: &aws_sdk_ecs::Client, cluster_arn: &str, service_name: &str) -> Result<String, EcsError> {
    let resp = client
        .list_tasks()
        .cluster(cluster_arn)
        .set_service_name(Some(service_name.into()))
        .send()
        .await?;

    let task_arns = resp.task_arns();
    println!("Found {} tasks:", task_arns.len());

    let tasks = client
        .describe_tasks()
        .cluster(cluster_arn)
        .set_tasks(Some(task_arns.into()))
        .send()
        .await?;

    for task in tasks.tasks() {
        if let Some(task_arn) = task.task_arn() {
            println!("  ARN: {}", task_arn);
            return Ok(task_arn.to_string());
        }
    }

    Ok("".to_string())
}

async fn list_services(
    client: &aws_sdk_ecs::Client,
    cluster_arn: &str,
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut all_service_arns = vec![];
    let mut next_token = None;

    // Collect all service ARNs with pagination
    loop {
        let resp = client
            .list_services()
            .cluster(cluster_arn)
            .set_next_token(next_token.clone())
            .send()
            .await?;

        // let service_arns = resp.service_arns();
        // all_service_arns.push(service_arns);
        let arns: Vec<String> = resp
            .service_arns()
            .iter()
            .cloned()
            .collect();

        all_service_arns.extend(arns);

        next_token = resp.next_token().map(|t| t.to_string());
        if next_token.is_none() {
            break;
        }
    }

    // Process chunks of services in parallel
    let chunks = all_service_arns.chunks(10);
    let mut tasks = vec![];

    for chunk in chunks {
        let client_clone = client.clone();
        let cluster = cluster_arn.to_string();
        let mut services: Vec<String> = vec![];
        for arns in chunk {
            services.push(arns.to_string());
        }

        let task = tokio::spawn(async move {
            let resp = client_clone
                .describe_services()
                .cluster(&cluster)
                .set_services(Some(services))
                .send()
                .await?;

            let service_names: Vec<String> = resp
                .services()
                .iter()
                .filter_map(|s| s.service_name().map(|n| n.to_string()))
                .collect();

            Ok::<Vec<String>, SdkError<DescribeServicesError>>(service_names)
        });

        tasks.push(task);
    }

    // Collect and flatten results
    let results = try_join_all(tasks).await?;

    // Handle any errors inside the Vec<Result<_, _>>
    let flattened: Vec<String> = results
        .into_iter()
        .collect::<Result<Vec<Vec<String>>, _>>()?  // Collect results, propagating errors
        .into_iter()
        .flatten()
        .collect();

    Ok(flattened)
}