---
id: find-out-which-archive
answers:
  problem_class: engram.start.wrong-build
  when:
    os.name: linux
severity: medium
proposes:
  - action: report_only
    because: >-
      Finding out which file you have is something you do in your own file
      manager or shell; this agent does not go looking through your downloads
---
**Engram does not start, and the first thing to settle is which archive you
have.** Every other answer here depends on it, and it is quicker to check than
to guess: the four downloads look alike and only one of them will run on this
machine.

This machine runs **Linux**. The archive you want is one of the two whose name
contains `linux` — `x86_64` on an ordinary desktop or laptop, `aarch64` on a
Raspberry Pi or another ARM board.

If you still have the file, its name is the answer. If you have already deleted
it and only the unpacked folder is left, ask the program itself:

    file ./engram

`ELF 64-bit ... x86-64` is the ordinary Linux build. `ELF 64-bit ... ARM
aarch64` is the ARM one. `PE32+ executable` means you took the Windows archive,
and `Mach-O` the macOS one — neither will ever start here, whatever you do to
the file permissions.

Once you know, start the diagnosis again and say which one it is; there is an
answer waiting for each.
