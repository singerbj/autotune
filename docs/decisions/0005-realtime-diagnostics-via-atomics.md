# 0005 — Audio threads report through atomics

**Status:** Accepted

## Context

The architecture routes "errors and log lines from audio threads" through an
SPSC ring drained by the control plane. Forming log lines on an audio thread
means formatting strings, which risks allocation.

## Decision

Audio threads never produce text. They publish:

- stream state and the failing HRESULT via `StreamStatus` atomics;
- xruns, underruns, overflows, fill levels and resampler ratio via
  `CallbackStats` atomics;
- the callback-duration histogram (10 µs buckets) via atomics;
- meters via a triple buffer.

The control loop (30 Hz) turns these into events, log lines (`tracing`) and
recovery actions. Discrete objects (processors, latency probes) still move
through `rtrb` queues.

## Consequences

Same observability, with nothing on an audio thread that can allocate.
