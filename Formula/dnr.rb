class Dnr < Formula
  desc "Shared Deno runtime and application packager"
  homepage "https://github.com/fansion314/dnr"
  url "https://github.com/fansion314/dnr/releases/download/v0.4.2/dnr-0.4.2-macos-arm64-r1.tar.gz"
  version "0.4.2"
  revision 1
  sha256 "bc260742d2accfc7335ee9c0681c881d3d015d2d77a0c5cb52efd84c5e88dd38"
  license "MIT"

  depends_on arch: :arm64
  depends_on macos: :sequoia

  def install
    bin.install "bin/dnr", "bin/dnc"
    doc.install "README.md", "README.zh.md", "THIRD_PARTY.md", "docs"
    (pkgshare/"licenses").install Dir["licenses/*"]
  end

  test do
    assert_match "dnr #{version}", shell_output("#{bin}/dnr --version")
    assert_match "dnc #{version}", shell_output("#{bin}/dnc --version")
    (testpath/"app/main.ts").write <<~TS
      const value: string = Deno.readTextFileSync(new URL("./data.txt", import.meta.url));
      console.log(`${value}:${Deno.args[0]}`);
    TS
    (testpath/"app/data.txt").write "homebrew"
    system bin/"dnc", "app", "--entry", "main.ts", "--app-id", "dev.dnr.homebrew.test", "-o", "app.dnp"
    assert_equal "homebrew:ok", shell_output("#{bin}/dnr app.dnp ok").strip
  end
end
