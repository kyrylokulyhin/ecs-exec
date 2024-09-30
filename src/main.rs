use clap::Parser;
use aws_config::meta::region::RegionProviderChain;
use aws_sdk_ecs::Client as EcsClient;
use aws_sdk_ecs::Error as EcsError;
use aws_sdk_ecs::config::Region as Region;
use std::error::Error;
use std::io::{self, Write};
use std::process::{Command, Stdio};
use std::io::Read;

/// A CLI tool to interactively run ECS `execute-command`
#[derive(Parser, Debug)]
#[command(author = "Kyrylo Kulyhin", version = "0.1.0", about = "ECS execute-command CLI tool", long_about = None)]
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

    let region_provider = RegionProviderChain::first_try(Region::new(aws_region.clone())).or_default_provider();

    let shared_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(region_provider)
        .profile_name(aws_profile.clone())
        .load()
        .await;
    let ecs_client = EcsClient::new(&shared_config);

    let cluster_arn = show_clusters(&ecs_client).await?;
    let task_arn = show_tasks(&ecs_client, &cluster_arn, &args.service).await?;

    let mut child = Command::new("aws")
        .arg("--profile")
        .arg(aws_profile)
        .arg("--region")
        .arg(aws_region)
        .arg("ecs")
        .arg("execute-command")
        .arg("--cluster")
        .arg(cluster_arn)
        .arg("--task")
        .arg(task_arn)
        .arg("--container")
        .arg(container_name)
        .arg("--interactive")
        .arg("--command")
        .arg(args.command)
        .stdin(Stdio::piped())  // Attach stdin for interactive input
        .stdout(Stdio::piped()) // Capture stdout for interactive output
        .stderr(Stdio::piped()) // Capture stderr for error handling
        .spawn()?;

    // Handles for child's stdin, stdout, and stderr
    let stdin = child.stdin.as_mut().expect("Failed to open stdin");
    let mut stdout = child.stdout.take().expect("Failed to open stdout");
    let mut stderr = child.stderr.take().expect("Failed to open stderr");

    // Spawn a thread to read from stdout and print to the user's console
    let stdout_thread = std::thread::spawn(move || {
        let mut stdout_buf = [0; 1024];
        let stdout = &mut stdout;

        loop {
            match stdout.read(&mut stdout_buf) {
                Ok(0) => break, // End of output
                Ok(n) => {
                    print!("{}", String::from_utf8_lossy(&stdout_buf[..n]));
                    io::stdout().flush().unwrap(); // Ensure output is printed immediately
                }
                Err(err) => {
                    eprintln!("Error reading stdout: {:?}", err);
                    break;
                }
            }
        }
    });

    // Spawn a thread to read from stderr and print errors to the console
    let stderr_thread = std::thread::spawn(move || {
        let mut stderr_buf = [0; 1024];
        let stderr = &mut stderr;

        loop {
            match stderr.read(&mut stderr_buf) {
                Ok(0) => break, // End of error output
                Ok(n) => {
                    eprint!("{}", String::from_utf8_lossy(&stderr_buf[..n]));
                    io::stderr().flush().unwrap(); // Ensure error output is printed immediately
                }
                Err(err) => {
                    eprintln!("Error reading stderr: {:?}", err);
                    break;
                }
            }
        }
    });

    // Reading input from the user and writing it to the command's stdin
    let mut input = String::new();
    while io::stdin().read_line(&mut input).unwrap() > 0 {
        stdin.write_all(input.as_bytes())?;
        stdin.flush()?;
        input.clear();
    }

    // Wait for stdout and stderr threads to finish
    stdout_thread.join().expect("Failed to join stdout thread");
    stderr_thread.join().expect("Failed to join stderr thread");

    // Wait for the command to finish
    let status = child.wait()?;
    println!("Command exited with status: {}", status);

    Ok(())
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
