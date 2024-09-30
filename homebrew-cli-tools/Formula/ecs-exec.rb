class EcsExec < Formula
  desc "CLI tool to execute commands in an AWS ECS container"
  homepage "https://github.com/kyrylokulyhin/ecs-exec"
  url "https://github.com/kyrylokulyhin/ecs-exec/releases/download/v0.1.0/ecs-exec-x86_64-apple-darwin.zip"
  sha256 "PUT_SHA256_CHECKSUM_OF_THE_ZIP_FILE_HERE"
  version "0.1.0"

  def install
    bin.install "ecs-exec"
  end

  test do
    system "#{bin}/ecs-exec", "--version"
  end
end