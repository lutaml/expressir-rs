**Status: READY TO RUN — validation task, no new code expected**

## What

Compile the full SMRL (schemas-smrl-all.yml, 1307 schemas / 43.3 MB)
into one artifact + overlays; measure cold/warm/memory; verify warm
byte-parity at that scale; record numbers.

## Expected

- Cold batch compile: ~35-60s wall on a quiet box (parse-dominated;
  the 7 largest mim_lf files are ~230s of serial parse but spread
  across workers).
- Warm: ~15-25s (hydration-bound; ~4x SRL's 4.1s).
- Peak RSS cold: largest-file arena + models ≈ 2.5-3 GB; warm ≈
  models only ≈ 2.4 GB.

## Watch for

- Overlay sizes at 1307 files (remarks/refs JSONs may reach tens of
  MB — fine, but record).
- The 7 giant files' worker wall-time — if one exceeds all others
  combined, consider TODO.expressir-rs/11 backend selection sooner.

## Run 2026-09-21 (COMPLETE — byte parity at full scale)

```
cold+write 405s (loaded box) | warm 105s | 1307 schemas
RSS 3.4GB | to_hash identical: TRUE
artifact 151MB | remarks 26.7MB | refs 95.1MB
```

- Warm parity holds with all three overlays + graph tables at full
  scale — the compiled set is production-ready for the corpus.
- Warm 105s is hydration-bound (magnus object building), consistent
  with the SRL breakdown scaled ~21x; attack via TODO 13 after #106.
- Sidecar totals ~123MB vs 151MB artifact — fine, but a single-file
  bundle is the natural TODO 14 revisit if disk footprint matters.
