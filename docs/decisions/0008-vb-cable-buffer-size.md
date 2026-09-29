# 0008 — VB-Cable internal buffer size

**Status:** Accepted

The architecture wants VB-Cable's internal buffer at 2048 samples and asks
whether it can be set without its control panel. VB-Audio documents no
supported registry or command-line switch, and writing undocumented driver
registry values from an installer is fragile. The app does not change it.
The cable writer adapts to whatever period the cable reports (PI-steered
resampler targeting two periods of fill), and the wizard mentions the VB-Cable
control panel for users who want to lower Discord-path latency.
