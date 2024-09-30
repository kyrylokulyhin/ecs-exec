# ECS Exec CLI

[![SWUbanner](https://raw.githubusercontent.com/vshymanskyy/StandWithUkraine/main/banner2-direct.svg)](https://github.com/vshymanskyy/StandWithUkraine/blob/main/docs/README.md)

Welcome to the **ECS Exec CLI** project! This tool is designed to interactively execute commands within an AWS ECS
container using the `ecs execute-command` functionality. It was developed as a learning project to explore **Rust**.

**Note**: This project is tailored to a specific workflow and setup, but it can be adapted and extended by other
developers.

---

## Features

- **Asynchronous Rust**: Powered by the Tokio runtime for handling concurrent tasks.
- **AWS SDK Integration**: Uses AWS SDK for Rust to interact with ECS clusters and tasks.
- **Custom Cluster Suffix Matching**: Specifically looks for ECS clusters ending with `-main`.
- **Execute Commands in ECS Containers**: Provides functionality to run any command within a specific ECS container.

---

## Prerequisites

Before using this tool, ensure you have the following set up:

1. **Rust**: Install Rust by following the official [Rust installation guide](https://www.rust-lang.org/tools/install).
2. **AWS CLI**: Ensure you have the AWS CLI installed and configured with proper permissions.
    - You can install the AWS CLI [here](https://docs.aws.amazon.com/cli/latest/userguide/install-cliv2.html).
3. **AWS Permissions**: The IAM role associated with your AWS CLI profile should have permissions for
   `ecs:DescribeClusters`, `ecs:ListTasks`, `ecs:ExecuteCommand`, and other related actions.

---

## Installation

1. Clone the repository:
   ```bash
   git clone https://github.com/yourusername/ecs-exec-cli.git
   ```

2. Navigate to the project directory:
   ```bash
    cd ecs-exec-cli
    ```

3. Build the project:

```bash
   cargo build --release
   ```

4. Run the executable:

```bash
   ./target/release/ecs-exec-cli --help
   ```

## Usage

### Run with Named and Positional Arguments

This CLI tool supports both named and positional arguments. You can specify the ECS cluster, task, and command either
with flags or positionally.

#### Basic Usage

```bash
./ecs-exec-cli --profile <AWS_PROFILE> --region <AWS_REGION> <SERVICE> <CONTAINER> <COMMAND>
```

#### Example:

```bash
./ecs-exec-cli --profile my-aws-profile --region us-east-1 my-service-name my-container-name "/bin/bash"
```

#### This command will:

- Connect to the specified ECS cluster and task.
- Open an interactive shell (/bin/bash) within the container.

#### Flags and Positional Arguments

- --profile (-p): The AWS CLI profile to use (defaults to dt-infra).
- --region (-r): The AWS region to use (defaults to eu-north-1).
- <SERVICE>: The name of the ECS service (positional).
- <CONTAINER>: The ECS container name (positional).
- <COMMAND>: The command to execute inside the container (positional).

#### Additional Notes

- The project currently looks for ECS clusters that end with -main. This is a specific setup related to the personal workflow of the author. Feel free to modify this to suit your needs.
- It also defaults to using app as the container name, which is configurable with the --container flag.

## Contributing

Since this project was built as part of a Rust learning journey, contributions are more than welcome! Whether you’re learning Rust or want to help improve the tool, feel free to:

 1.	Fork the repository.
 2. Create a new branch (git checkout -b feature/my-new-feature).
 3.	Commit your changes (git commit -am 'Add some feature').
 4.	Push to the branch (git push origin feature/my-new-feature).
 5.	Open a pull request.

### Areas to Improve

-	Support for more flexible ECS cluster matching (not tied to -main).
-	Error handling improvements.
-	More robust interaction with AWS services.
-	Adding unit and integration tests.

### License

This project is licensed under the MIT License. See the LICENSE file for details.

### Acknowledgements

Thanks to the Rust and AWS SDK communities for providing excellent documentation and resources to get started!