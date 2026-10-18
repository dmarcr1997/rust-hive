# FieldHive control terminal

The Hive controller now serves its command center at `http://ZIMA_IP:8080/`.
The interface uses the approved manila-paper / dark-instrument-panel design,
Windows 95 bitmap typography, and pixel terminal text. Fonts and icons are
bundled in the controller; there are no CDN requests, frontend build steps,
or additional production services.

## Controls

- **Network:** select a registered node in the topology. CPU and RAM values
  come from real agent heartbeats; offline or unverified telemetry displays
  a dash. The topology adapts to the actual registered nodes.
- **Infer:** send a prompt to the selected node's configured model. Enabled
  only for nodes advertising `Inference`. The controller does not discover
  model names or claim that a model is loaded just because its agent is online.
- **Echo / Check:** test a round trip to the selected agent. Check confirms
  agent connectivity; it does not test Ollama. The scan button beside the
  selected hardware runs the same check.
- **Write / Read:** store and retrieve observations on a Tools/Storage node.
  Observations stay on that node, using the agent's existing `observations.jsonl`.
- **Run record:** replay the latest three responses or failures. Up to twenty
  entries and the completion count are retained in this tab's session storage.
  This is not a server audit log.
- **Copy:** copy the returned text, with selection as a fallback where browser
  clipboard access is unavailable. **Ctrl+Enter** also submits a task.

Tasks use `POST /nodes/{node_id}/execute`, so another node with matching
capabilities cannot receive a task intended for the selected node. Missing,
offline and unsupported targets return 404, 503 and 403 respectively. Failed
agent requests and invalid/mismatched results return 502; timeouts return 504.
The existing automatic `POST /tasks/execute` route remains available.

The terminal starts empty and reads `/nodes` every five seconds. It contains
no simulated nodes, hardcoded telemetry or canned model responses. While a
request is running, node selection and other submissions are locked. Browser
requests time out after 200 seconds; controller dispatch is limited to 195
seconds. A timeout does not guarantee cancellation of work already sent to
an agent, so inspect the agent before retrying a write.

## Deploy on the existing ZimaBoard

After merging the pull request, pull the updated default branch in the existing
ZimaBoard checkout:

```bash
git pull --ff-only
docker build -f Dockerfile.control -t hive-control:latest .
```

Use the `Dockerfile.control` already present in your deployment. Its source-copy
step must include the whole controller directory, for example
`COPY hive-control ./hive-control`, so Rust can embed `src/dashboard.html`.
This repository does not currently track that deployment-specific Dockerfile.

After the build succeeds, recreate the controller with the same settings as
before. For the existing deployment:

```bash
docker stop hive-control
docker rm hive-control
docker run -d --name hive-control --restart unless-stopped \
  -p 8080:8080 hive-control:latest
```

Restart the running agents once so they register with the new controller. Its
registry is currently in memory, and existing agents do not re-register when
heartbeats receive 404. Keep Ollama and the existing SSH forwarding running.
Then open `http://ZIMA_IP:8080/` in a browser on the same network.

For a native deployment, `cargo run --release -p hive-control --locked` serves
the same interface. No agent or protocol upgrade is needed for this PR.

## Demo

1. Select `thinkcentre-llm` and click the scan/check button.
2. Choose **Infer**, enter a short prompt, and click **Run**.
3. Select a Tools/Storage node. Choose **Write** and store a field observation.
4. Choose **Read** to retrieve it, then use **Copy** if desired.
5. Return to the inference node and explicitly paste notes into a summary prompt.

The terminal does not automatically forward observations to the model.

## Verification

```bash
cargo test --workspace --locked
cargo build --release -p hive-control --locked
```

Optional DOM interaction checks require a compatible Node.js version
(`^22.22.2`, `^24.15.0`, or `>=26.0.0`) only on the development machine:

```bash
cd hive-control/ui-tests
npm ci --ignore-scripts
npm test
```

The seven Rust route tests cover HTML serving, exact node selection, target
errors, automatic routing compatibility, mismatched responses, invalid upstream
responses and timeouts. The eight DOM tests cover live tasks, safe text
rendering, observation reads/writes, offline/reconnection behavior, busy guards,
drafts, history, malformed responses, timeouts and bundled resources.

Browser visual/layout QA was unavailable in the build environment. After
deployment, check the topology, fonts and narrow-screen layout in your browser.

## Operating boundary

Use the existing trusted LAN setup: Hive's API currently has no authentication.
This PR does not add a shell, process management, automatic model discovery or
unattended reboot recovery. Do not expose the existing API to the public Internet.
Asset licenses are recorded in `THIRD_PARTY_NOTICES.md` and embedded in the HTML.
