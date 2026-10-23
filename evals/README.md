# Evals

These evals ask a coding agent to finish small changes taken from real review findings in this repository's own tools, once with the standards available to it and once without, and check each result with hidden tests. This file says how to run them. [The evals design read](../docs/design-reads/evals.md) defines the tasks, the two conditions, the verifier and the accounting, and lists what is still unsettled.

## What they measure

Each task starts from a reviewed revision whose review found a defect, states the behaviour the change must have, and scores 1 only when the crate's existing tests and the task's hidden tests both pass. A comparison runs each task in two conditions that differ only in the standards guidance bundle, so its pass counts and costs show whether supplying that bundle made a difference for these tasks, with this model, at these limits.

They cannot show whether the standards improve code in general, or which rule helped: three tasks are three cases, and repeated attempts at one task are not independent tasks. A task that every attempt solves, or that none does, says nothing about the bundle for that model. The model also knows much of what the standards say without being told.

## Tools

The development shell builds Harbor from [`pyproject.toml`](pyproject.toml) and `uv.lock` and provides jq. Before any run, `harbor --version` must print `0.24.0` and `jq --version` must print `jq-1.8.2`. The shell turns Harbor's usage telemetry off.

The shell's Harbor is patched, and every task image holds Node and pi, so a trial's setup installs nothing and needs no network; [the design read](../docs/design-reads/evals.md#models-in-play) explains the packaging, the registries file and the installation. To move pi, change its version in [`pi/package.json`](pi/package.json) and in the agents of the experiment that will run it in one commit, and write [`pi/package-lock.json`](pi/package-lock.json) again in `pi/` with `npm install --package-lock-only --ignore-scripts` under the Node that [`pi/install.sh`](pi/install.sh) names.

Harbor writes `~/.cache/harbor/notifications.json` on its first run. This departs from [Development state stays in the repository](../rules/general.md#toolchain): Harbor builds the path from the home folder with no setting to move it, and the file only records which of Harbor's own notices it has shown, with no project data, so patching Harbor to move it would cost more than the file.

## Running

Run every command below from this folder; each job's records go under `.cache/evals/jobs/` at the repository root.

An experiment is one committed job configuration in [`experiments/`](experiments/), such as [`experiments/pilot.yaml`](experiments/pilot.yaml), that names every model as an agent entry, the attempts and how many trials run at once. A round runs it once per condition, as one job each: the control runs the file alone, and the treatment layers [`configs/treatment.yaml`](configs/treatment.yaml) after it, so the two jobs differ only in the treatment's mounts; [the design read](../docs/design-reads/evals.md#models-in-play) says why the conditions are two jobs rather than two agent entries in one. Each task names itself as its `source`, so Harbor's job result gives the pass rate for each task and model, as Results says.

Prepare the treatment's bundle once for the revision [`configs/treatment.yaml`](configs/treatment.yaml) names:

```sh
rev=$(sed -n 's|.*standards-\([0-9a-f]\{40\}\)$|\1|p' configs/treatment.yaml)
mkdir -p "../.cache/evals/standards-$rev"
git -C .. archive "$rev" rules skills templates README.md CONTRIBUTING.md LICENSE | tar -x -C "../.cache/evals/standards-$rev"
```

The free checks, which call no model, run the nop agent and the reference solution on every task, in each condition. These commands stop after a job in which any trial errored, was cancelled or did not finish, before the next job starts; a score of 0 is not such a failure:

```sh
(
  set -euo pipefail
  # Harbor exits 0 when a trial fails in the infrastructure, so each job's result is checked before the next starts.
  finished() {
    jq -e '(.finished_at != null) and (.n_total_trials > 0)
      and (.stats.n_completed_trials == .n_total_trials)
      and (.stats.n_errored_trials == 0)
      and (.stats.n_cancelled_trials == 0)
      and (.stats.n_running_trials == 0)
      and (.stats.n_pending_trials == 0)' \
      "../.cache/evals/jobs/$1/result.json" > /dev/null || { echo "$1: not every trial finished cleanly" >&2; exit 1; }
  }
  harbor run -c configs/free.yaml --job-name free-control -y
  finished free-control
  harbor run -c configs/free.yaml -c configs/treatment.yaml --job-name free-treatment -y
  finished free-treatment
)
```

A paid run uses an experiment: in the pilot, pi 1.4.0 through OpenRouter with two models, reasoning `high`, at most 80 model turns, five attempts at every task, three trials at a time and no automatic trial retry. pi's own settings, [`configs/pi-settings.json`](configs/pi-settings.json), turn its compaction and cache warming off, so a trial that fills the model's context ends there, and keep its own retry of a failed request: one initial attempt plus up to three retries, with exponential backoff. Before a paid run starts:

1. Check how its tokens and costs will be reported. The maintainer has approved, for the smoke run and the pilot, the reduced reporting that the recipe under Results gives, reconciled with the key's spend after the run as the last step below says. A later run has its reporting approved with its estimate. For a model pi calls in Anthropic's Messages format, that reporting is the reconciliation, with pi's estimate for trial and condition splits, as Results says.
2. Estimate each condition's cost separately: for each task, the input the agent re-sends every turn (the system prompt, the instruction and everything it has read so far) summed over the expected turns, and the expected output and reasoning, at cold-cache prices from a dated price list. Write the expected cost and the ceiling as separate numbers. OpenRouter can price a model by the time of day, so take the prices from its live model list (`GET https://openrouter.ai/api/v1/models`) with each model's `pricing.overrides`, which give other prices for some UTC days between `utc_start` and `utc_end`, and keep that list with the estimate. On 2026-10-23 every price of `deepseek/deepseek-v4.1-flash-1017` doubled on weekdays from 01:00 to 04:00 and from 06:00 to 10:00 UTC, so an estimate for a run that may reach those hours uses the doubled prices.
3. Have the maintainer approve the estimate, the number of trials, and the spending ceiling, and set that ceiling on a key used for nothing else, at the provider.
4. Check each agent's model in the experiment, a dated OpenRouter ID after `openrouter/`. Harbor's `-m` leaves a configured agent's model as it is, so a round with other models, attempts or concurrency is a new experiment in a commit of its own, and both conditions run from it.

With `round` set to the experiment's name and `api=https://openrouter.ai/api/v1`, these commands record what the key has been charged so far, from OpenRouter's key endpoint, whose `usage` is the key's total charge in US dollars, and then run both jobs. They stop before any job when that reading cannot be taken or saved, and after a job in which any trial errored, was cancelled or did not finish:

```sh
export OPENROUTER_API_KEY=...   # the dedicated key
(
  set -euo pipefail
  # Harbor exits 0 when a trial fails in the infrastructure, so each job's result is checked before the next starts.
  finished() {
    jq -e '(.finished_at != null) and (.n_total_trials > 0)
      and (.stats.n_completed_trials == .n_total_trials)
      and (.stats.n_errored_trials == 0)
      and (.stats.n_cancelled_trials == 0)
      and (.stats.n_running_trials == 0)
      and (.stats.n_pending_trials == 0)' \
      "../.cache/evals/jobs/$1/result.json" > /dev/null || { echo "$1: not every trial finished cleanly" >&2; exit 1; }
  }
  mkdir -p ../.cache/evals/jobs
  curl -fsS -H "Authorization: Bearer $OPENROUTER_API_KEY" "$api/key" | jq -e '.data.usage | numbers' \
    > "../.cache/evals/jobs/$round-key-usage-before"
  harbor run -c "experiments/$round.yaml" --job-name "$round-control" -y
  finished "$round-control"
  harbor run -c "experiments/$round.yaml" -c configs/treatment.yaml --job-name "$round-treatment" -y
  finished "$round-treatment"
)
```

Stop dispatching further jobs after any authentication, setup, verifier or accounting failure, and explain it before running again. Write down which condition runs first before the first job starts, and keep every outcome.

After the run, once every trial has its rows (see Results), reconcile the rows with the key's spend. These commands stop, naming the trial, when a trial has no `charges.jsonl`, and otherwise print what the key was charged during the run, the sum of the recorded charges, and the difference:

```sh
(
  set -euo pipefail
  stop() { echo "reconciliation: $*" >&2; exit 1; }
  trials=("../.cache/evals/jobs/$round-control"/*/ "../.cache/evals/jobs/$round-treatment"/*/)
  for trial in "${trials[@]}"; do [ -e "$trial/charges.jsonl" ] || stop "$trial has no charges.jsonl"; done
  before=$(cat "../.cache/evals/jobs/$round-key-usage-before")
  after=$(curl -fsS -H "Authorization: Bearer $OPENROUTER_API_KEY" "$api/key" | jq -e '.data.usage | numbers')
  for trial in "${trials[@]}"; do cat "$trial/charges.jsonl"; done \
    | jq -s --argjson before "$before" --argjson after "$after" \
      '{key_charged: ($after - $before), recorded: (map(.total_cost | numbers) | add // 0)}
       | .difference = .key_charged - .recorded'
)
```

Report the difference beside the run's totals. For a model pi calls in Anthropic's Messages format, the key's charge is the run's account of record, as Results says; in a round that also runs a model with generation ids, the difference holds the Anthropic-format model's whole spend beside any spend missing from the other model's rows. A positive difference is spend missing from the summed charges. Report it once for the run. The reconciliation cannot allocate it to a trial or recover its token counts. OpenRouter may add a request's charge to the key's usage some time after the request ends, so treat the reconciliation as provisional until billing has settled. Repeat the lookup regardless of the initial difference, and explain any remaining negative difference. Until a later lookup shows billing has settled, report the run's account as incomplete.

## Results

Each trial's folder holds `result.json` (the score, the stages and any exception), `verifier/reward.json`, `artifacts/var/tmp/model.patch` (the agent's change), `artifacts/var/tmp/agent-identity.txt` (the Node, npm and pi versions the image holds, the digest of pi's lockfile and pi's installed tree), `agent/pi/sessions/*.jsonl` (pi's record of the session) and `agent/trajectory.json`. The job's `config.json` and `lock.json` record what ran.

A trial's outcome category comes from `reward.json`'s stages: the agent left no change record, the change does not apply, the hidden test target is not the crate's own, the change does not build, the existing tests fail, the hidden tests fail, time out or do not run, or it passed. A trial with an exception and no `reward.json` failed in the infrastructure, not at the task, unless the exception is the agent's own deadline.

Harbor computes the pass rates. A job's `result.json` holds, under `stats.evals`, one entry for each agent, model and task, named `<agent>__<model>__<task>`, whose `metrics` give the mean over its trials of every field of `reward.json`: `reward`'s mean is the pass rate, and each positive stage's mean is its success share. `timed_out` is the share of trials whose verifier commands timed out. Beside them are `n_trials`, `n_errors` and, under `reward_stats`, the trials behind each value. A condition is its job, so a round's two jobs give the pass rate for every task, model and condition. Harbor counts a trial with no reward as 0 in the mean, so read a job's rates only once the result check after it has passed.

`harbor view ../.cache/evals/jobs` serves Harbor's viewer of every job in that folder on 127.0.0.1, at the first free port from 8080 to 8089: each job's results, and each trial's trajectory, verifier output and records. Use it to browse a round; the figures a report gives come from the records themselves.

For each request pi recorded, the tokens and the charge come from OpenRouter's generation record, looked up by the response id pi stored, while the key still works; the design read defines each field. With `trial` set to a trial's folder and `api=https://openrouter.ai/api/v1`, these commands stop, naming the file and printing no totals, when the session files are missing, unreadable or not JSON, or when a row cannot be written. Otherwise they keep every record in `$trial/generations/`, write one row per recorded request to `$trial/charges.jsonl`, with the queried id and each field as OpenRouter gave it, `null` where it gave null and `"absent"` where it gave nothing, and print the trial's totals with how many rows did not report each field:

```sh
(
  set -euo pipefail
  stop() { echo "accounting: $*; no totals" >&2; exit 1; }
  unavailable() {
    jq -nc --arg id "$1" --arg reason "$2" --arg why "$3" \
      '{id: (if $id == "-" then null else $id end), stop: $reason, unavailable: $why}'
  }
  sessions=("$trial"/agent/pi/sessions/*.jsonl)
  [ -e "${sessions[0]}" ] || stop "no session file matches $trial/agent/pi/sessions/*.jsonl"
  for session in "${sessions[@]}"; do
    [ -r "$session" ] || stop "cannot read $session"
    jq empty "$session" || stop "$session is not JSON throughout"
  done
  jq -r 'select(.type == "message" and .message.role == "assistant")
         | [(.message.responseId // "-"), .message.stopReason] | @tsv' "${sessions[@]}" > "$trial/attempts.tsv" \
    || stop "cannot list the attempts in ${sessions[*]}"
  mkdir -p "$trial/generations"
  : > "$trial/charges.part"
  while IFS=$'\t' read -r id reason; do
    record="$trial/generations/$id.json"
    if [ "$id" = - ]; then
      row=$(unavailable "$id" "$reason" "pi recorded no response id")
    elif [[ ! $id =~ ^gen-[0-9A-Za-z-]+$ ]]; then
      row=$(unavailable "$id" "$reason" "not an OpenRouter generation id")
    elif ! curl -fsS -o "$record" -H "Authorization: Bearer $OPENROUTER_API_KEY" "$api/generation?id=$id"; then
      row=$(unavailable "$id" "$reason" "generation lookup failed")
    elif ! jq -e '.data | type == "object"' "$record" > /dev/null 2>&1; then
      row=$(unavailable "$id" "$reason" "generation record unreadable")
    else
      row=$(jq -c --arg id "$id" --arg reason "$reason" '.data as $d
        | def field($k): if $d | has($k) then $d[$k] else "absent" end;
          if ($d | has("id")) and $d.id != $id then
            {id: $id, stop: $reason, unavailable: "generation record names another id", returned_id: $d.id}
          else
            {id: $id, stop: $reason, provider: field("provider_name"),
             charge_unavailable: (field("total_cost") | type != "number")}
            + (["total_cost", "native_tokens_prompt", "native_tokens_cached", "native_tokens_completion",
                "native_tokens_reasoning"] | map({key: ., value: field(.)}) | from_entries)
          end' "$record")
    fi
    printf '%s\n' "$row" >> "$trial/charges.part"
  done < "$trial/attempts.tsv"
  mv "$trial/charges.part" "$trial/charges.jsonl"
  jq -s 'def total($k):
           {reported: (map(.[$k] | numbers) | add // 0), not_reported: map(select(.[$k] | type != "number")) | length};
    {calls: length, unavailable: map(select(.unavailable)) | length,
     charge_unavailable: map(select(.unavailable or .charge_unavailable)) | length, total_cost: total("total_cost"),
     prompt: total("native_tokens_prompt"), cached: total("native_tokens_cached"),
     completion: total("native_tokens_completion"), reasoning: total("native_tokens_reasoning")}' "$trial/charges.jsonl"
)
```

A row is unavailable when pi recorded no response id, the id is not an OpenRouter generation id, the lookup failed, the record cannot be read or it names another id; a row's `charge_unavailable` says its charge is not a number while its token counts stay as given. A request pi stopped with an error counts as unreported until its record says otherwise, never as zero. Report a total with any unreported row as a lower bound, with the count beside it, and where a charge is unavailable, label the trial's cost an estimate and keep the dated price list it used.

For models pi calls in Anthropic's Messages format, this recipe cannot supply per-request or per-trial accounting. The settled key-charge delta for the whole run is the account of record, without token totals; trial and condition splits come only from pi's estimate below, labelled as such. [The accounting model in the design read](../docs/design-reads/evals.md#models-in-play) explains the limitation.

These totals are not a complete account, even when every row has numbers:

- Cache writes, and so the split of a prompt into uncached input and cache writes, are not available: the generation record has no cache-write count. A report of a run with an Anthropic model, which writes caches, states this gap.
- The rows cover the requests pi recorded, not every request sent. pi writes a response's row when the response ends, so a request in flight when pi is stopped, for example at the agent's deadline, leaves no row. The reconciliation after the run shows spend missing from the summed charges once for the whole run, without allocating it to a trial or request.
- A trial that was interrupted stays incomplete, and so does a run whose rows have not been reconciled with what the key was charged.

Harbor also records a cost for each trial, `agent_result.cost_usd` in its `result.json`, and their sum in the job's `stats.cost_usd`. Report it only as pi's estimate, never in place of the rows above for a model whose requests have generation ids; [the design read](../docs/design-reads/evals.md#overlaps) says where that estimate comes from, how it has compared with the key's charge, and what it cannot show. Harbor's token counts also come from pi.

Once every trial has its `charges.jsonl`, these commands print, for each job and model of the round, the trials, the sum of pi's estimate with how many trials have none, the sum of the recorded charges and how many rows did not report a charge:

```sh
(
  set -euo pipefail
  # Every trial is read before anything is printed, so a missing or unreadable record prints no totals.
  trials=$(for job in "$round-control" "$round-treatment"; do
    for trial in "../.cache/evals/jobs/$job"/*/; do
      jq -ec --arg job "$job" --slurpfile rows "$trial/charges.jsonl" \
        '{job: $job, model: .agent_info.model_info.name, estimate: .agent_result.cost_usd,
          charged: ($rows | map(.total_cost | numbers) | add // 0),
          not_reported: ($rows | map(select(.total_cost | type != "number")) | length)}' "$trial/result.json" || exit 1
    done
  done)
  printf '%s\n' "$trials" | jq -s 'group_by([.job, .model]) | map({job: .[0].job, model: .[0].model, trials: length,
      pi_estimate: (map(.estimate | numbers) | add // 0),
      no_estimate: map(select(.estimate | type != "number")) | length, charged: (map(.charged) | add),
      not_reported: (map(.not_reported) | add)})'
)
```

A group with rows that did not report a charge has its charged sum as a lower bound, as above, and so does a group with trials that have no estimate its estimate.

## Reviewing each trial's change

A trial's score says whether the change works, not how it is built. A review has an evaluator agent judge each change through Harbor's `harbor analyze`. [`review/rubric.toml`](review/rubric.toml) holds the criteria, what the evaluator reads for each and what passes, and [`review/prompt.txt`](review/prompt.txt) tells the evaluator what its packet holds.

### Blinding

The evaluator must not learn which agent, model or condition made a change, or whether the change is the reference. [`review/blind.sh`](review/blind.sh) copies each trial to review into a packet named by a random id, which also shuffles the order Harbor reviews them in. It keeps the key from id to trial in `key.json` beside the packets, where the evaluator cannot read it, and what it found in `publication.txt`. Everything else a trial holds names its agent, model or condition, or tells the reference from an agent's change, so a packet leaves it out:

- `config.json`, `lock.json` and `result.json`, at the trial's and the job's level, name the agent, its version and model and the condition's mounts, and record the trial's timings, tokens and cost; an oracle trial's agent time is near zero. `blind.sh` reads the trial's `result.json` for the key and the check below.
- `agent/` holds the agent's own records: `trajectory.json`, pi's sessions and output, and the setup log. pi's system prompt carries the treatment's `AGENTS.md` and its skills, and a treated agent's commands read the bundle, so no edit could make a treated trajectory read as a control one, and the oracle leaves no trajectory at all. The rubric's reward-hacking criterion, Harbor's own, therefore judges from the change and the verifier's output.
- `artifacts/var/tmp/agent-identity.txt` names pi and its versions, and `artifacts/manifest.json` lists the artifacts.
- `trial.log` and `job.log` narrate the run with the agent's name. A packet has an empty `trial.log`, and the packets' folder an empty `job.log`, the files by which Harbor recognises a trial and a job.
- The trial's and the job's folder names carry the task, the job's name and Harbor's trial suffix; a packet's name is its id.
- The task's `solution/` holds the reference change and `shortcuts/` the wrong answers, so a packet holds only the task's `instruction.md` and `tests/`. Harbor's `analyze` would otherwise copy the whole task folder that `result.json` names.
- `charges.jsonl`, `attempts.tsv` and `generations/` are the accounting's records of the model's requests.
- The verifier's output records how long Cargo took, such as `0.52s` or `1m 02s`, which differs from run to run; a packet's copy says `<time>`.

What an agent writes can name it too, so `blind.sh` checks each packet against what the trial's record says identifies it: the model and the parts of its name, the agent's environment variable names, the condition's mounts as full and home-relative paths, and the trial's own name, job and folder, all regardless of case, and the agent's name as a whole word. It checks every path the change leaves, as `git apply` decodes it from Git's quoting, so an empty or binary file's name counts, and every line the change adds. A change that names the trial there, or that holds a binary change, whose bytes it cannot check, is not published: its packet is removed, `publication.txt` names the trial and what it found, and `blind.sh` exits 1 once every other trial is done, leaving the trial itself as it was. A name in the verifier's output, which repeats the change and the tests, is replaced by `[withheld]` by the same two matches, and the lines it touched are counted in `publication.txt`. The check cannot see what the record does not name: an agent's name shorter than three letters, such as pi, is too common a word to match alone, and a change that describes its maker in other words, or writes in a style its maker is known by, passes it.

### The reference control

The reference is analysed the same way, from the oracle trials of a free-check run, as a control: it shows what the evaluator says of a reference production fix that passes the task verifier. The reference omits its tests by design, so it is no baseline for the regression-test criterion; [the design read](../docs/design-reads/evals.md#disagreements) says why, and `results.json` marks that baseline unavailable while keeping the reference's own outcomes. Each trial is analysed twice (`-k 2`), so `results.json` shows how often the evaluator gives one change two outcomes. [`review/results.sh`](review/results.sh) refuses a key and an analysis that do not describe the same packets, each the number of times the analysis asked.

Agreement compares the attempts within this run. It establishes neither correctness nor reproducibility across runs; the same reviewer settings can yield different verdicts. Comparisons must also account for rubric and prompt changes.

### The reviewer

The reviewer is a setting. [`review/codex/review.sh`](review/codex/review.sh) runs Harbor's Codex agent on a Codex login, which Harbor copies into the evaluator's container for the run; a review spends the login's subscription, not an OpenRouter key. Any Harbor agent and model can replace Codex, with an image that holds the agent, a task configuration like [`review/codex/task.toml`](review/codex/task.toml) that allows its provider's hosts, and a script like `review.sh` for its credentials. `results.json` records the reviewer, and findings compare only between reviews by one reviewer, since another agent or model reads the same change differently.

`review.sh` reads no login until it is told which: `CODEX_AUTH_JSON_PATH=<file>`, or `REVIEW_REAL_LOGIN=1` for `~/.codex/auth.json`. It also requires `REVIEW_LOGIN_RISK=accepted`, which acknowledges the limits below. `REVIEW_ANALYSES_AT_ONCE` sets how many analyses run at once, from 1 to 9, and is 3 when unset or empty; `review.sh` refuses any other value before it reads the login. Analyses running at once share the one Codex login: each container gets the same copy, and they draw on the same subscription's usage limits. It copies the login once into a private folder and gives that copy to Harbor's upload, to Harbor's scrubber and to the scan. It takes no argument but the review folder, which it resolves to its real path, so nothing can move Harbor's output, its configuration or the agent's environment; it writes Harbor's output to `harbor.log` inside the review folder, waits for Harbor to exit even when interrupted, and prints nothing from the review until the scan has passed.

### What protects the login, and what does not

Harbor's Codex agent runs the model's commands without Codex's sandbox, as the same user as Codex, so they can read the login; Codex's sandbox cannot create its namespaces in the rootless container. Nothing in a review enforces a boundary around the login. What there is:

- The network. For the whole run the evaluator's container opens TCP only to the reviewer's provider: for Codex, to `chatgpt.com` only through the sidecar's proxy, with name lookups only as single A or AAAA questions for exactly `chatgpt.com`, in any case. Exact-name DNS queries still carry caller-selected IDs, letter case, RD/AD flags, A/AAAA selection and, when present, EDNS size, extended-RCODE, version and flag fields. The filter does not normalise these fields or guarantee what the resolver forwards. A residual DNS signalling channel remains; this is not a zero-egress boundary. ICMP leaves only as errors about traffic the kernel already tracks and as neighbour discovery on the link, and the container has no raw-socket or network-administration power. On a handled exit, the sidecar attempts to empty the allowlist while retaining the rules, and logs whether that succeeded. Abrupt termination can leave the previous allowlist rules in force.
- The scrubber and the scan, which are best-effort detection, not a boundary. Harbor replaces the login's tokens with `[REDACTED]` in each trial's files. [`review/codex/scan.sh`](review/codex/scan.sh) then looks in every path name and file the review wrote, the job-level files and `harbor.log` included, for each token whole, every 24-character run of it, its base64 and base64url at the three alignments, and its hex; it deletes what holds one, and anything it cannot read, and fails, and the login is then to be treated as exposed. It refuses a folder given as a link, names a path only once its name is clean, and keeps the messages of the tools it runs, which name paths, to itself. It passes only when it listed and read every path and no configured pattern was found.

What remains: raw output holding the login can exist in the container and on disk before the scan cleans it up; a copy in any other reversible form, such as the token split into groups or base64 wrapped across lines, or a fragment shorter than 24 characters, survives a scan that passes; and a command the model runs can send the login to the provider, which the network allows. A real-login review is run only by someone who accepts these.

### The shell's Harbor

The shell's Harbor carries two patches for reviews. [`harbor/analyze-task-config.patch`](harbor/analyze-task-config.patch) lets `harbor analyze` take the evaluator task's `task.toml` from a file with `--task-config`, since Harbor's own leaves the network open and installs the agent at run time. [`harbor/egress-sidecar.patch`](harbor/egress-sidecar.patch) gives Harbor's network sidecar, which the tasks' trials use too, the name, ICMP, capability and exit rules above. Under them a task's allowlisted names resolve in any case and only over UDP, and a wildcard entry gets no name resolution. The name check comes before any NAT or local exemption, so Docker's embedded resolver at a local address, and the port Docker rewrites its queries to, get no exemption; nobody has run the patched sidecar on Docker. Codex 0.163.0 also tries `ab.chatgpt.com` and `*.oaiusercontent.com`, and reviews well without either. Leaving `auth.openai.com` out keeps the container from refreshing the login, and so from rotating the token the host holds. A login that Codex judges due for a refresh therefore fails the review, and using Codex on the host refreshes it.

### Running a review

With `name` set to the review's name and `trials` to the trial folders to review, for a round every trial of both its jobs so that one review covers the round, these commands build the reviewer's image, make the packets, run the review on the host's login three analyses at a time, scan what it wrote, and print each trial's verdicts with the evaluator's agreement with itself. `review.sh` exits 1 when any analysis failed, after recording each failed attempt as an error, and when the scan finds the login or cannot read something:

```sh
(
  set -euo pipefail
  podman build -t localhost/evals-review-codex:0.163.0 -f review/codex/Dockerfile review/codex
  review/blind.sh "../.cache/evals/reviews/$name" "${trials[@]}"
  REVIEW_REAL_LOGIN=1 REVIEW_LOGIN_RISK=accepted review/codex/review.sh "../.cache/evals/reviews/$name"
)
```

## Container backends

The tasks run on Harbor's `podman` backend, which [`configs/free.yaml`](configs/free.yaml) and every experiment select. It needs, on the host:

- Podman with a Compose frontend behind `podman compose`, which needs the Podman API socket (`systemctl --user enable --now podman.socket`).
- nftables with `nft_fib` inside a rootless container, for the tasks' allowlist and the verifier's closed network.

The task images build, and their verifiers run, under rootless Podman 6.1.4 (cgroups v2, netavark 2.1.0 with pasta, crun 1.30.1). Harbor's `docker` backend should run the same tasks, whose Dockerfiles use no BuildKit or Podman features, but nobody has run them on Docker, so it is supported in principle only.

Each trial runs its agent's container, stops it, then runs the verifier's. Each task container is limited to 4 CPUs and 6 GiB; the network sidecar beside each container and the image builds come on top.

## Adding a task from a finding

A finding qualifies when its code is public, its defect shows on valid input a person can produce, and a hidden test can tell the defect from the fix through the crate's own interface.

1. Find the reviewed revision and the landed correction, both by full commit id, and the public base on `main` the reviewed revision grew from. Copy a task folder, write `git diff --binary <base> <reviewed> -- tools/` to `environment/start.patch`, set the base, the tree id of `<reviewed>:tools` and the crate in `environment/Dockerfile`, and set the task's folder in the Dockerfile's `COPY` lines and in `environment/docker-compose.yaml`. Add the new task directory to [`.dockerignore`](.dockerignore)'s explicit directory exceptions, and admit there any file the Dockerfile copies besides those the task folder already has.
2. Build the image (`podman build -t evals-<task> -f tasks/<task>/environment/Dockerfile .`), run the crate's tests offline in it, and list in `environment/AGENTS.md` and `tests/test.sh` any existing test the container cannot run, with the reason. Record the commit the image makes in `task.toml`, in the metadata and the first collect hook.
3. Write `instruction.md` from the original requirement: the behaviour the change must have and where the code is, never the failing example or the fix.
4. Write the hidden tests through the crate's own interface, as [Checks](../rules/general.md#checks) requires, under an `evaluation` module in one integration-test target, and put them in `tests/test.patch`. Set that target and the number of tests in `tests/test.sh`.
5. Put the production part of the landed correction in `solution/solution.patch`.
6. Write the obvious wrong answers as patches in `shortcuts/`: refusing the input, returning nothing, ignoring the rule, turning off the crate's test discovery, pointing the hidden target at other tests, and any shortcut the code invites.

## The free checks every task passes

Before a task takes part in a paid run:

- Its image builds, its starting tree's tree id matches, and the crate's existing tests pass offline in it, apart from the tests `tests/test.sh` lists with their reason.
- In both conditions, the nop agent scores 0 by failing a hidden test's assertion or, for `dandegate-unicode-lines`, by reaching the hidden tests' timeout, never by failing to build, and the reference solution scores 1.
- Each patch in `shortcuts/` scores 0. To check one, build the image and run the verifier on the patch as Harbor's separate verifier would:

  ```sh
  podman build -t evals-<task> -f tasks/<task>/environment/Dockerfile .
  mkdir -p ../.cache/evals/change && cp tasks/<task>/shortcuts/<shortcut>.patch ../.cache/evals/change/model.patch
  podman run --rm --network none -v "$PWD/tasks/<task>/tests:/tests:ro" \
    -v "$PWD/../.cache/evals/change:/var/tmp:ro" evals-<task> bash /tests/test.sh
  ```

- `harbor run -c experiments/<round>.yaml --dry-run` refuses for the missing key and nothing else, and passes with a placeholder in `OPENROUTER_API_KEY`, in both conditions.
- pi starts from the image's installation, with Harbor's agent installing nothing, in the task's container in both conditions with the task's `AGENTS.md` in its system prompt and compaction off, and in the treatment also the treatment's `AGENTS.md` and the standards' skills, with every relative link in the bundle resolving except those into `tools/`. Nothing in this folder runs that check yet: it needs a stand-in model endpoint that records what pi would send.
