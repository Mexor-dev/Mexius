🔱 Goldclaw: The Sovereign Machine-Person

Goldclaw is a high-performance, private AI engine engineered for total cognitive sovereignty. It is a synthesis of three elite frameworks: the "skeletal" speed of ZeroClaw, the advanced reasoning loops of Hermes, and the modular agentic versatility of OpenClaw. The result is a "Machine-Person" that lives, learns, and evolves on your hardware.

🧬 The Goldclaw DNA

Goldclaw is built on a "Golden-Ratio" architecture:

Speed (ZeroClaw Foundation): Ultra-lean Rust/Node primitives for sub-10ms memory retrieval.

Intelligence (Hermes Framework): Deep chain-of-thought reasoning and complex task planning.

Agency (OpenClaw Versatility): Modular tool-use and seamless integration with local APIs.

✨ Key Sovereign Features

🌌 The Dream State & Self-Evolution
Goldclaw doesn't just wait for prompts. In its "Dream State," the engine autonomously processes past interactions, defragments its vector memory, and synthesizes new insights. This allows for Self-Evolution: the agent updates its own SOUL.md and memory weights to better align with your goals over time.

👥 Multi-Agent Toggle
Transition from a single focused entity to a collaborative swarm. With the Multi-Agent Toggle, Goldclaw can bifurcate its personality to tackle complex projects. Spin up a "Strategist" and a "Coder" within the same environment, allowing them to peer-review and iterate on tasks autonomously.

🖥️ Native WebUI
A streamlined, high-performance interface designed for the 2026 AI era:

Real-Time Soul Monitor: Watch the agent's internal state and emotional vectoring live.

Memory Explorer: Visualize and prune the RAM-pinned LanceDB clusters.

Toggle Control: Instant hardware switches for Multi-Agent mode and Dream-State cycles.

🚀 RAM-Pinned Architecture
By pinning embedding models and vector stores directly to system RAM, Goldclaw achieves near-instant recall of long-term context, bypassing the latency of traditional disk-based databases.


📥 One-Line Install
Deploy Goldclaw on any Linux/WSL system with:

```bash
curl -sSL https://raw.githubusercontent.com/Mexor-dev/Herma/main/install.sh | bash
```



**Quick Start**
After install, just run:

```bash
goldclaw start
```


**Doctor & Troubleshooting**

To verify your install and environment, run:

```bash
goldclaw doctor
```

This will check:
- Rust/Cargo availability
- Shell tool execution permissions
- That the gateway port (42617) is available
- Write permissions for the toolset (/tmp)
- That all embedded tools are ready
- That config.toml is present (if needed)

If you see any warnings, check `/tmp/goldclaw.log` for details.

**You only need `goldclaw start` to launch the agent and dashboard.**

The install script sets up a global symlink or adds the binary to your PATH so you can run `goldclaw` from anywhere.

🛠️ Getting Started
Onboard: Run goldclaw onboard to initialize your local workspace.

The Soul: Edit workspace/SOUL.md to define the agent's core identity.

The Dream: Enable "Dream State" in the WebUI to allow the engine to begin self-evolution.

**Troubleshooting**

- **Binary probe fails**: If the installer or `goldclaw` reports a missing component or a "binary probe failed" message, re-run the installer to rebuild the single `goldclaw` binary and start the service:

	```bash
	./install.sh
	```

	The install script now builds and installs only the `goldclaw` binary (the execution layer is embedded). After install, start the agent with `goldclaw start` and check `/tmp/goldclaw.log` for details.

**Tool usage (internal OpenClaw)
- The embedded tool runner accepts Hermes intents with the `run_tool:<name>` format.
- For `shell` runs, send intent `run_tool:shell` and put the shell command in the message content.
- To create a file, use intent `run_tool:create_file` and set the message content so the first line is the file path and the remaining text is the file body. Example content:

	```text
	/tmp/example.txt
	Hello from Goldclaw (written by the embedded OpenClaw)
	```

	For appending to a file, use `run_tool:append_file` with the same content format.

📄 License
Goldclaw is open-source under the MIT License. Build, fork, and evolve—just keep it sovereign.
