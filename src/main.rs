mod pty;

use clap::Parser;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ecs::Client as EcsClient;
use aws_sdk_ecs::Error as EcsError;
use aws_sdk_ecs::config::Region as Region;
use aws_sdk_sts::Client as StsClient;
use aws_sdk_sts::operation::get_caller_identity::GetCallerIdentityError;
use std::error::Error;
use std::io::{self, Write};
use std::process::{Command};

/// A CLI tool to interactively run ECS `execute-command`
#[derive(Parser, Debug)]
#[command(author = "Kyrylo Kulyhin", version = "0.1.3", about = "ECS execute-command CLI tool", long_about = None
)]
struct Cli {
    /// The AWS profile to use
    #[arg(long, short = 'p', default_value = "dt-infra")]
    profile: String,

    // The AWS region to use
    #[arg(long, short = 'r', default_value = "eu-north-1")]
    region: String,

    /// The ECS service name
    #[arg()]
    service: String,

    /// The container name in the ECS task
    #[arg(default_value = "app")]
    container: String,

    /// The command to run inside the container
    #[arg(default_value = "bash")]
    command: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Cli::parse();

    let aws_region = args.region.clone();
    let aws_profile = args.profile.clone();
    let container_name = args.container.clone();


    match check_sso_session(&aws_profile).await {
        Ok(_) => {
            // Proceed with ECS client operations if the session exists
            // let region_provider = RegionProviderChain::first_try(Region::new(aws_region.clone()))
            //     .or_default_provider();

            // let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            //     .region(region_provider)
            //     .profile_name(aws_profile.clone())
            //     .load()
            //     .await;

            println!("SSO session is active. Proceeding with ECS operations for container: {}", container_name);
        }
        Err(_) => {
            if prompt_user_for_login() {
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
            } else {
                println!("Exiting the program.");
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
    let task_arn = show_tasks(&ecs_client, &cluster_arn, &args.service).await?;

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

fn prompt_user_for_login() -> bool {
    println!("No active AWS SSO session found.");
    println!("Please run the following command to login via AWS SSO:");
    println!("  aws sso login");
    print!("Would you like to retry after logging in? (Y/n): ");
    io::stdout().flush().unwrap(); // Make sure prompt gets printed immediately

    let mut input = String::new();
    io::stdin().read_line(&mut input).expect("Failed to read input");
    let input = input.trim().to_lowercase();

    input == "y" || input == "yes"
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
