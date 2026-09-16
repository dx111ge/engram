---
id: wrong-build-on-an-intel-machine
answers:
  problem_class: engram.start.wrong-build
  when:
    os.name: linux
    os.arch: x86_64
    engram.download: engram-linux-aarch64.zip
severity: high
proposes:
  - action: report_only
    because: >-
      The fix is downloading a different file, which is yours to do — this
      agent does not fetch or unpack archives, and a support tool that
      installed software would be a different kind of program
---
**Engram does not start at all.** No window, or an error saying the file cannot
be executed — `cannot execute binary file: Exec format error` in a terminal. It
is not a configuration problem and there is nothing to fix in your setup: the
download you unpacked is built for a different kind of processor than the one in
this machine.

This machine is **x86_64** (Intel/AMD). The archive you have is the **aarch64**
(ARM) build, the one for a Raspberry Pi or another ARM board. Both are Linux
builds and their names differ only at the end, which is why this is the easiest
of the four downloads to pick by mistake.

## What to do

Download `engram-linux-x86_64.zip` from the releases page instead, unpack it,
and start it the same way you tried before.

**Your knowledge base is not affected.** A `.brain` file is the same on both
builds — one created by the ARM build opens in the Intel build and the other way
round. If you already made one, keep it.

**There is nothing to clean up.** The archive you have never ran, so it changed
nothing on this machine. Delete the unpacked files if you like, or leave them.

## Why there is no error message worth reading

Engram ships as a single binary with no runtime dependencies, which is what
makes it easy to install — and also what makes this failure complete. There is
no loader to complain, no runtime to report a mismatch, and no emulation layer
to fall back on. The operating system is handed a program built in an
instruction set this processor does not speak, and there is nothing further to
say about it.
