---
id: find-out-which-archive
answers:
  problem_class: engram.start.wrong-build
  # No conditions at all, and that is what makes it the answer everything else
  # falls back to: a question may only be asked once something applies above it,
  # so the class needs one answer that holds before anything has been read or
  # asked. Conditioned on the operating system it was reached first and answered
  # in place of the three below it; conditioned on the download it sat *under*
  # the question and the question had nothing above it.
  when: {}
severity: medium
proposes:
  - action: report_only
    because: >-
      Finding out which file you have is something you do in your own file
      manager or shell; this agent does not go looking through your downloads
---
**Engram does not start, and which archive you have decides everything else
here.** There is an answer waiting for each of them, and none of them can be
given until that is settled — the four downloads look alike and only one of
them runs on any given machine.

If you still have the file, its name is the answer. If you have deleted it and
only the unpacked folder is left, ask the program itself.

On Linux or macOS:

    file ./engram

`ELF 64-bit ... x86-64` is the ordinary Linux build and `ELF 64-bit ... ARM
aarch64` the one for a Raspberry Pi or another ARM board. `Mach-O` is the macOS
build, and `PE32+ executable` the Windows one.

On Windows, right-click `engram.exe`, choose **Properties** and look at
**Details**: a file that is not a Windows program has no version information
there at all, which is itself the answer.

Then start the diagnosis again and say which one it is.

If you have checked and the archive really is the one for this machine, then
this is not the wrong build and there is no answer written up for it yet —
which is worth telling us, because that is how one gets written.
