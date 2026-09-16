---
id: wrong-archive-for-this-system
answers:
  problem_class: engram.start.wrong-build
  when:
    os.name: linux
    engram.download: [engram-windows-x86_64.zip, engram-macos-aarch64.zip]
severity: high
proposes:
  - action: report_only
    because: >-
      The fix is downloading a different file, which is yours to do — this
      agent does not fetch or unpack archives, and a support tool that
      installed software would be a different kind of program
---
**Engram does not start at all.** Nothing opens, or the shell says the file is
not executable, or `cannot execute binary file`. There is nothing wrong with
your setup and nothing to configure: the archive you unpacked is built for a
different operating system than the one you are running.

This machine runs **Linux**. The archive you named is built for **Windows** or
**macOS**, and those are not interchangeable — a Windows `.exe` is not a program
Linux can start, whatever the file permissions say.

Take `engram-linux-x86_64.zip` from the releases page instead, or
`engram-linux-aarch64.zip` if this machine is an ARM board rather than an
ordinary PC. Unpack it somewhere of your own and run it from there; nothing has
to be uninstalled first, because the archive you have never ran.

If you are unsure which of the two Linux archives you need, the answer is
`x86_64` on any ordinary desktop or laptop, and `aarch64` on a Raspberry Pi, an
Apple-silicon machine running Linux, or a similar ARM board.
