---
id: wrong-archive-on-windows
answers:
  problem_class: engram.start.wrong-build
  when:
    os.name: windows
    engram.download: [engram-linux-x86_64.zip, engram-linux-aarch64.zip, engram-macos-aarch64.zip]
severity: high
proposes:
  - action: report_only
    because: >-
      The fix is downloading a different file, which is yours to do — this
      agent does not fetch or unpack archives, and a support tool that
      installed software would be a different kind of program
---
**Engram does not start at all.** Nothing opens, or Windows says the file is
not a valid application. There is nothing wrong with your setup and nothing to
configure: the archive you unpacked is built for a different operating system
than the one you are running.

This machine runs **Windows**, and the file you named is a Linux or macOS
build. Neither will ever start here, whatever you do to it — they are different
kinds of file, not a version or a permission problem.

Download **`engram-windows-x86_64.zip`** from the releases page and unpack it
somewhere else, so the two do not end up mixed in one folder. Nothing you have
stored is affected: your `brain` is a separate file and the new build opens it
unchanged.
