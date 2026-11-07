class Globw < Formula
  desc "Statically-typed language with its own syntax and a fast Rust engine, in a single file"
  homepage "https://globw.org"
  version "0.1.3"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/Globw/globw/releases/download/v0.1.3/globw-macos-aarch64.tar.gz"
      sha256 "d0c95480a7093aa8d7e9680746633810b6b569640222a13b54f5eb3ba6d2d6b2"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/Globw/globw/releases/download/v0.1.3/globw-linux-x86_64.tar.gz"
      sha256 "8b5259473004c78d5a4e0997b675ef5abb1f96cea18b68246c0f094290e3c419"
    end
    on_arm do
      url "https://github.com/Globw/globw/releases/download/v0.1.3/globw-linux-aarch64.tar.gz"
      sha256 "87ccf42a58be4a3923971b86a73310977b695b5fa1f66a3fe61965bb903cb2ad"
    end
  end

  def install
    bin.install "gw", "gwget"
  end

  test do
    # `gw` with no arguments prints its usage and exits 3
    assert_match "Usage", shell_output("#{bin}/gw 2>&1", 3)
  end
end
