---
id: wrong-build-for-this-machine
answers:
  problem_class: engram.start.wrong-build
  when:
    os.arch: aarch64
    engram.download: engram-linux-x86_64.zip
severity: high
proposes:
  - action: report_only
    because: >-
      The fix is downloading a different file, which is yours to do — this
      agent does not fetch or unpack archives, and a support tool that
      installed software would be a different kind of program
---
This machine is aarch64 and the archive you downloaded is the x86_64 build.
Engram ships as a single binary with no runtime dependencies, which is what
makes it easy to install and also what makes this failure total: there is no
loader, no shim and no emulation layer to fall back on, so the binary simply
does not start.

Download `engram-linux-aarch64.zip` from the releases page instead. Your
`.brain` file is not affected — it is the same format on both, so a brain
created by one build opens in the other.

Nothing else needs undoing. The x86_64 archive never ran, so it left nothing
behind except the files you unpacked.
