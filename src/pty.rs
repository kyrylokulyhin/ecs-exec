use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::thread;
use termios::{Termios, TCSANOW, ECHO, ICANON, ISIG};
use std::os::unix::io::AsRawFd;

pub(crate) fn spawn_pty_shell(cmd: &str, args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    // Create a new PTY system
    let pty_system = NativePtySystem::default();
    let pair = pty_system.openpty(PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    })?;

    // Save the original terminal attributes of stdin
    let stdin_fd = std::io::stdin().as_raw_fd();
    let orig_termios = Termios::from_fd(stdin_fd)?;

    // Set stdin to raw mode
    let mut raw_termios = orig_termios.clone();
    raw_termios.c_lflag &= !(ECHO | ICANON | ISIG);
    termios::tcsetattr(stdin_fd, TCSANOW, &raw_termios)?;

    // Ensure that stdin is set back to original mode when the program exits
    let _termios_guard = TermiosGuard::new(stdin_fd, orig_termios.clone());

    // Build your command that connects to the ECS task
    // Replace this with your actual command and arguments

    let mut cmd_builder = CommandBuilder::new(cmd);
    cmd_builder.args(args);
    cmd_builder.env(
        "TERM",
        std::env::var("TERM").unwrap_or_else(|_| "xterm-256color".to_string()),
    );

    // Spawn the child process within the PTY
    let mut child = pair.slave.spawn_command(cmd_builder)?;

    // Close the slave side of the PTY in the parent process
    drop(pair.slave);

    // Set up reader and writer for the PTY master
    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;

    // Spawn a thread to handle input from stdin to the PTY
    thread::spawn(move || {
        let stdin = std::io::stdin();
        let mut stdin = stdin.lock();
        let mut buffer = [0u8; 1024];
        loop {
            let n = stdin.read(&mut buffer).expect("Failed to read from stdin");
            if n == 0 {
                break;
            }
            writer
                .write_all(&buffer[..n])
                .expect("Failed to write to PTY");
            writer.flush().expect("Failed to flush PTY writer");
        }
    });

    // Read output from the PTY and write to stdout
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    let mut buffer = [0u8; 1024];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        stdout.write_all(&buffer[..n])?;
        stdout.flush()?;
    }

    // Wait for the child process to exit
    child.wait()?;

    Ok(())
}

// Helper struct to restore terminal attributes on exit
struct TermiosGuard {
    fd: i32,
    termios: Termios,
}

impl TermiosGuard {
    fn new(fd: i32, termios: Termios) -> Self {
        TermiosGuard { fd, termios }
    }
}

impl Drop for TermiosGuard {
    fn drop(&mut self) {
        let _ = termios::tcsetattr(self.fd, TCSANOW, &self.termios);
    }
}