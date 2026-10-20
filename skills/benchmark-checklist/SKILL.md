---
name: benchmark-checklist
description: "Vet a performance measurement for its limiter, tuning, physical limits, errors, repeatability, relevance, and whether the work happened before you report or act on it. Use when you run a benchmark or report a measured speedup or regression."
---

# Benchmark checklist

Use this checklist when you produce a performance number, such as a pull request's before-and-after comparison, a regression claim, an optimisation loop, or a library or configuration choice. Answer each question with evidence from a run, not a guess about the code.

For a quick estimate the user asked for, one run is enough. Still check questions 4 and 7, and state that the number comes from one run. Skip the remaining questions unless that run looks wrong. A choice between options is never a quick estimate.

## Before you run anything

- Write down the claim you expect to make in the exact words you would publish, such as "export is 30% faster at p50 on the 60k-row dataset". The questions below test that sentence.
- Read the measurement script. Note what it times, counts, and ignores.
- Check the load average with `uptime` and the core count with `nproc`. If the machine is busy, find out what is running. If you cannot stop it, interleave the sides so both see the same noise, and say so in the report.

## Answer the seven questions

1. **Why not double?** Name the limiter. Profile in a separate run that you do not report, because profilers and tracers slow the work down. Measure CPU per process with `top` or `pidstat`, run a profiler such as `node --cpu-prof`, `py-spy`, or `perf`, check I/O wait, and count syscalls with `strace -c` on Linux, then map the hot spot to source. Check the load generator too. If it saturates first, you measured the load generator. If a change did not move the number, find the limiter before you call the change useless.
2. **Was it tuned?** Run every side the way production runs it: release builds, production flags and environment variables, batching and transaction settings, connection pools, caches as warm or cold as production sees them, and the same versions and data. If one side runs on defaults, you compared configurations, not implementations. A limiter caused by a setting, such as a commit per row, a debug build, or a missing index, means that side is untuned. Tune it and measure again before you pick a winner. If you cannot tune it, do not pick a winner from that run. When the user is choosing what to adopt, narrowing the claim to today's shipping configuration does not fix this, because they adopt the option rather than today's settings.
3. **Did it break limits?** Do the arithmetic. Compare bytes per second with disk and network bandwidth, operations per second times cost per operation with your cores, and the time saved with the time the changed piece took. Removing a piece that takes 10% of the run can make the run at most about 11% faster. A result past a limit measured something other than the work, such as a cache, a no-op, or a bug.
4. **Did it error?** Count failures and non-success responses, and check that outputs are correct, not just present. Rejections are often fast, and timeouts and retries are slow. If the script does not count errors, add the count.
5. **Does it reproduce?** Run each side at least five times, alternating A and B so warmup, lazy initialisation, caches, and drift favour neither side. Report the median and range. A gap smaller than the run-to-run spread is not a measurable difference. When the result is close, use a rank-sum test or the benchmark tool's own statistics.
6. **Does it matter?** Next to any micro result, measure the end-to-end path a user waits on, with realistic data sizes and concurrency, and report the micro result as a share of the whole. A helper that takes 1% of a request can make the request at most 1% faster, however fast the helper gets.
7. **Did it even happen?** Confirm that the work ran inside the timed region: the request reached the server, the database wrote the rows, the reader consumed the bytes, and the caller used the result. Timeouts and lazy code, such as unconsumed generators, unawaited promises, or dead results the JIT discards, produce fast numbers for work that never happened.

## Report the measurement

- Lead with the verdict: faster, slower, no measurable difference, or inconclusive.
- Give the number with its unit, the run count, the range, and the limiter, such as "p50 fell from 41 ms to 33 ms, median of 7 runs per side, range 32 to 35 ms after, bound by JSON parsing on one core."
- Call the verdict inconclusive when you claim a difference but cannot name the limiter, when a side ran untuned, or when you could not check questions 4 and 7. Name the gap.
- Keep a pull request body to one primary number. Put the runs, range, and limiter evidence in a linked artefact or notes file.

