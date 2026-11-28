class Globw < Formula
  desc "Statically-typed language with its own syntax and a fast Rust engine, in a single file"
  homepage "https://globw.org"
  version "0.1.7m"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/Globw/globw/releases/download/v0.1.7m/globw-macos-aarch64.tar.gz"
      sha256 "df2d411e8223d8f83a6e8c2112fae366f386b88468baad07d336e833f5edb48c"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/Globw/globw/releases/download/v0.1.7m/globw-linux-x86_64.tar.gz"
      sha256 "c20ec24fe926133e3e681a578752ba8794fe62e6703165f4634a7aaa5d8c55cf"
    end
    on_arm do
      url "https://github.com/Globw/globw/releases/download/v0.1.7m/globw-linux-aarch64.tar.gz"
      sha256 "90422086efbf68ebe190cc11b6f9d028a504e23d75a993c87a3a0ca99f575f97"
    end
  end

  def install
    bin.install "gw", "gwget"
    # syntax highlighting for nano, vim and micro
    pkgshare.install "editors" if File.directory?("editors")
  end

  test do
    # `gw` with no arguments prints its usage and exits 3
    assert_match "Usage", shell_output("#{bin}/gw 2>&1", 3)
  end
end
