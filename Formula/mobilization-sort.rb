class MobilizationSort < Formula
  desc "A humorous TUI visualizer for Partial Mobilization Sort"
  homepage "https://github.com/RUdolf1517/MOBSORT"
  url "https://github.com/RUdolf1517/MOBSORT.git", tag: "v0.1.2"
  version "0.1.2"
  license "MIT"

  depends_on "rust" => :build
  depends_on "pkgconf" => :build
  depends_on "alsa" if OS.linux?

  def install
    system "cargo", "install", "--path", ".", "--root", prefix
  end

  test do
    assert_predicate bin/"mobilization-sort", :exist?
  end
end
