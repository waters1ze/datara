class Datara < Formula
  desc "High-performance Post-OOP systems language & Forgen compiler"
  homepage "https://github.com/waters1ze/datara"
  version "0.1.0"
  license any_of: ["Apache-2.0", "MIT"]

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/waters1ze/datara/releases/download/v0.1.0/forgen-darwin-arm64.tar.gz"
      sha256 "30f25726ad838553b2b31029889f90cd50ae5ca3c61ebdeb97f61e07b877ed02"
    else
      url "https://github.com/waters1ze/datara/releases/download/v0.1.0/forgen-darwin-x64.tar.gz"
      sha256 "30f25726ad838553b2b31029889f90cd50ae5ca3c61ebdeb97f61e07b877ed02"
    end
  end

  on_linux do
    url "https://github.com/waters1ze/datara/releases/download/v0.1.0/forgen-linux-x64.tar.gz"
    sha256 "30f25726ad838553b2b31029889f90cd50ae5ca3c61ebdeb97f61e07b877ed02"
  end

  def install
    bin.install "forgen"
    bin.install_symlink "forgen" => "datara"
    pkgshare.install Dir["stdlib/*"]
  end

  test do
    (testpath/"test.dtr").write <<~EOS
      fn main() {
        println("Hello from Homebrew Datara!")
      }
    EOS
    assert_match "Hello from Homebrew Datara!", shell_output("#{bin}/forgen run test.dtr")
  end
end
