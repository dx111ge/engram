---
id: wrong-archive-on-macos
answers:
  problem_class: engram.start.wrong-build
  when:
    os.name: macos
    engram.download: [engram-linux-x86_64.zip, engram-linux-aarch64.zip, engram-windows-x86_64.zip]
severity: high
proposes:
  - action: report_only
    because: >-
      The fix is downloading a different file, which is yours to do — this
      agent does not fetch or unpack archives, and a support tool that
      installed software would be a different kind of program
---
**Engram does not start at all.** Nothing opens, or the shell says
`cannot execute binary file`, or macOS reports that the application cannot be
opened. There is nothing wrong with your setup and nothing to configure: the
archive you unpacked is built for a different operating system than the one you
are running.

This machine runs **macOS**, and the file you named is a Linux or Windows
build. Neither will ever start here, whatever you do to the file permissions —
`chmod +x` on a Linux binary changes nothing, because the format itself is not
one macOS can load.

Download **`engram-macos-aarch64.zip`** from the releases page and unpack it
somewhere else, so the two do not end up mixed in one folder. Nothing you have
stored is affected: your `brain` is a separate file and the new build opens it
unchanged.
