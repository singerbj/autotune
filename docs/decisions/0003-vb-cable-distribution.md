# 0003 — VB-Cable distribution

**Status:** Accepted

## Context

FR-15 wants the installer to install VB-Cable silently. Bundling the driver,
and pre-trusting VB-Audio's certificate, needs a distribution license from
VB-Audio that has not been secured yet (ARCHITECTURE › Licenses).

## Decision

- `installer/resources/install-vbcable.ps1` implements the full FR-15 flow.
- By default the driver pack is **downloaded from VB-Audio at install time**
  (the "link-out install" mitigation in the risk table). The script verifies
  the setup program's Authenticode signature before running it.
- Builds licensed to bundle set the repository variable `BUNDLE_VBCABLE=true`;
  the release workflow then places the pack in the installer.
- The installer records `HKLM\Software\VoiceTuner\InstalledVBCable=1` and the
  uninstaller offers to remove VB-Cable only when that flag is set.

## Consequences

Installs need network access unless the pack is bundled. The user still sees
one UAC prompt (the per-machine installer) and one reboot.
