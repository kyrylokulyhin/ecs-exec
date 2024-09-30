class EcsExec < Formula
  desc "CLI tool to execute commands in an AWS ECS container"
  homepage "https://github.com/kyrylokulyhin/ecs-exec"
  url "https://github.com/kyrylokulyhin/ecs-exec/releases/download/v0.1.0/ecs-exec-x86_64-apple-darwin.zip"
  sha256 "e7227a052a2d6d2817fb76ed6cdb0bc4631211937ec82e61c324f9ddde1977f3"
  version "v0.1.0"

  def install
    bin.install "ecs-exec"
  end

  test do
    system "#{bin}/ecs-exec", "--version"
  end
end