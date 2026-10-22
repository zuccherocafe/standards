# Evals

These evals ask a coding agent to finish small changes taken from real review findings in this repository's own tools, once with the standards available to it and once without, and check each result with hidden tests. This file says how to run them. [The evals design read](../docs/design-reads/evals.md) defines the tasks, the two conditions, the verifier and the accounting, and lists what is still unsettled.

## What they measure

Each task starts from a reviewed revision whose review found a defect, states the behaviour the change must have, and scores 1 only when the crate's existing tests and the task's hidden tests both pass. A comparison runs each task in two conditions that differ only in the standards guidance bundle, so its pass counts and costs show whether supplying that bundle made a difference for these tasks, with this model, at these limits.

They cannot show whether the standards improve code in general, or which rule helped: three tasks are three cases, and repeated attempts at one task are not independent tasks. A task that every attempt solves, or that none does, says nothing about the bundle for that model. The model also knows much of what the standards say without being told.

## Tools

The development shell builds Harbor from [`pyproject.toml`](pyproject.toml) and `uv.lock` and provides jq. Before any run, `harbor --version` must print `0.24.0` and `jq --version` must print `jq-1.8.2`. The shell turns Harbor's usage telemetry off.

Harbor writes `~/.cache/harbor/notifications.json` on its first run, a path it takes from the home folder with no setting to move it.

## Before the first run

Two things stop every Harbor run of these tasks on Podman until the maintainer decides them:

- Harbor's egress sidecar, which enforces the tasks' network policies, is built from the short image name `gogost/gost`, and its two scripts carry the Nix interpreter path the packaging writes into them, which the sidecar's image lacks.
- The tasks allow only `openrouter.ai` from start to teardown, while Harbor's pi agent downloads Node and pi during each trial's setup, so a pi trial's setup fails. The nop agent and the reference solution install nothing.

The design read's disagreements say what each choice costs.

## Running

Run every command below from this folder; each job's records go under `.cache/evals/jobs/` at the repository root.

Prepare the treatment's bundle once for the revision [`configs/treatment.yaml`](configs/treatment.yaml) names:

```sh
rev=$(sed -n 's|.*standards-\([0-9a-f]\{40\}\)$|\1|p' configs/treatment.yaml)
mkdir -p "../.cache/evals/standards-$rev"
git archive "$rev" rules skills templates README.md CONTRIBUTING.md LICENSE | tar -x -C "../.cache/evals/standards-$rev"
```

The free checks, which call no model, run the nop agent and the reference solution on every task, in each condition:

```sh
harbor run -c configs/free.yaml --job-name free-control -y
harbor run -c configs/free.yaml -c configs/treatment.yaml --job-name free-treatment -y
```

A paid run uses [`configs/pi.yaml`](configs/pi.yaml): pi 1.4.0 through OpenRouter, reasoning `high`, at most 80 model turns, one trial at a time and no automatic trial retry. pi's own settings, [`configs/pi-settings.json`](configs/pi-settings.json), turn its compaction and cache warming off, so a trial that fills the model's context ends there, and keep its own retry of a failed request: one initial attempt plus up to three retries, with exponential backoff. Before a paid run starts:

1. Have the maintainer approve how its tokens and costs will be reported. The recipe under Results cannot give a complete account (see its limits there), so a paid run needs either an approved method that captures or reconciles every request's usage and charge, or the maintainer's explicit approval of the reduced reporting the recipe gives. Without one of these, stop here.
2. Estimate each condition's cost separately: for each task, the input the agent re-sends every turn (the system prompt, the instruction and everything it has read so far) summed over the expected turns, and the expected output and reasoning, at cold-cache prices from a dated price list. Write the expected cost and the ceiling as separate numbers.
3. Have the maintainer approve the number of trials and the spending ceiling, and set that ceiling on a key used for nothing else, at the provider.
4. Check the model in `configs/pi.yaml`, a dated OpenRouter ID after `openrouter/`. Harbor's `-m` leaves a configured agent's model as it is, so a comparison with another model changes that line in a commit of its own, and both conditions run from it.

```sh
export OPENROUTER_API_KEY=...   # the dedicated key
harbor run -c configs/pi.yaml -k <attempts> --job-name <name>-control
harbor run -c configs/pi.yaml -c configs/treatment.yaml -k <attempts> --job-name <name>-treatment
```

Stop dispatching further jobs after any authentication, setup, verifier or accounting failure, and explain it before running again. For a comparison of several attempts, alternate the conditions in an order written down before the first job, and keep every outcome.

## Results

Each trial's folder holds `result.json` (the score, the stages and any exception), `verifier/reward.json`, `artifacts/var/tmp/model.patch` (the agent's change), `artifacts/var/tmp/agent-identity.txt` (the Node, npm and pi versions and the npm tree the trial installed), `agent/pi/sessions/*.jsonl` (pi's record of the session) and `agent/trajectory.json`. The job's `config.json` and `lock.json` record what ran.

A trial's outcome category comes from `reward.json`'s stages: the agent left no change record, the change does not apply, the hidden test target is not the crate's own, the change does not build, the existing tests fail, the hidden tests fail, time out or do not run, or it passed. A trial with an exception and no `reward.json` failed in the infrastructure, not at the task, unless the exception is the agent's own deadline.

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

A row is unavailable when pi recorded no response id, the lookup failed, the record cannot be read or it names another id; a row's `charge_unavailable` says its charge is not a number while its token counts stay as given. A request pi stopped with an error counts as unreported until its record says otherwise, never as zero. Report a total with any unreported row as a lower bound, with the count beside it, and where a charge is unavailable, label the trial's cost an estimate and keep the dated price list it used.

These totals are not a complete account, even when every row has numbers:

- Cache writes, and so the split of a prompt into uncached input and cache writes, are not available: the generation record has no cache-write count.
- The rows cover the requests pi recorded, not every request sent. pi writes a response's row when the response ends, so a request in flight when pi is stopped, for example at the agent's deadline, leaves no row.
- A trial that was interrupted, or whose rows have not been reconciled with what the key was charged, stays incomplete.

Harbor's own token counts and `cost_usd` are not the source; the design read says why.

## Container backends

The tasks run on Harbor's `podman` backend, which [`configs/free.yaml`](configs/free.yaml) and `configs/pi.yaml` select. It needs, on the host:

- Podman with a Compose frontend behind `podman compose`, which needs the Podman API socket (`systemctl --user enable --now podman.socket`).
- nftables with `nft_fib` inside a rootless container, for the tasks' allowlist and the verifier's closed network.
- A `registries.conf` that resolves the short image name `gogost/gost` to Docker Hub.

The task images build, and their verifiers run, under rootless Podman 6.1.4 (cgroups v2, netavark 2.1.0 with pasta, crun 1.30.1). Harbor's `docker` backend should run the same tasks, whose Dockerfiles use no BuildKit or Podman features, but nobody has run them on Docker, so it is supported in principle only.

Each trial runs its agent's container, stops it, then runs the verifier's. Each task container is limited to 4 CPUs and 6 GiB; the network sidecar beside each container and the image builds come on top.

## Adding a task from a finding

A finding qualifies when its code is public, its defect shows on valid input a person can produce, and a hidden test can tell the defect from the fix through the crate's own interface.

1. Find the reviewed revision and the landed correction, both by full commit id, and the public base on `main` the reviewed revision grew from. Copy a task folder, write `git diff --binary <base> <reviewed> -- tools/` to `environment/start.patch`, and set the base, the tree id of `<reviewed>:tools` and the crate in `environment/Dockerfile`.
2. Build the image, run the crate's tests offline in it, and list in `environment/AGENTS.md` and `tests/test.sh` any existing test the container cannot run, with the reason. Record the commit the image makes in `task.toml`, in the metadata and the first collect hook.
3. Write `instruction.md` from the original requirement: the behaviour the change must have and where the code is, never the failing example or the fix.
4. Write the hidden tests through the crate's own interface, as [Checks](../rules/general.md#checks) requires, under an `evaluation` module in one integration-test target, and put them in `tests/test.patch`. Set that target and the number of tests in `tests/test.sh`.
5. Put the production part of the landed correction in `solution/solution.patch`.
6. Write the obvious wrong answers as patches in `shortcuts/`: refusing the input, returning nothing, ignoring the rule, turning off the crate's test discovery, pointing the hidden target at other tests, and any shortcut the code invites.

## The free checks every task passes

Before a task takes part in a paid run:

- Its image builds, its starting tree's tree id matches, and the crate's existing tests pass offline in it, apart from the tests `tests/test.sh` lists with their reason.
- In both conditions, the nop agent scores 0 by failing a hidden test's assertion, not by failing to build, and the reference solution scores 1.
- Each patch in `shortcuts/` scores 0. To check one, build the image and run the verifier on the patch as Harbor's separate verifier would:

  ```sh
  podman build -t evals-<task> tasks/<task>/environment
  mkdir -p ../.cache/evals/change && cp tasks/<task>/shortcuts/<shortcut>.patch ../.cache/evals/change/model.patch
  podman run --rm --network none -v "$PWD/tasks/<task>/tests:/tests:ro" \
    -v "$PWD/../.cache/evals/change:/var/tmp:ro" evals-<task> bash /tests/test.sh
  ```

- `harbor run -c configs/pi.yaml --dry-run` refuses for the missing key and nothing else, and passes with a placeholder in `OPENROUTER_API_KEY`, in both conditions.
- pi starts in the task's container in both conditions with the task's `AGENTS.md` in its system prompt and compaction off, and in the treatment also the treatment's `AGENTS.md` and the standards' skills, with every relative link in the bundle resolving except those into `tools/`. Nothing in this folder runs that check yet: it needs a stand-in model endpoint that records what pi would send.
