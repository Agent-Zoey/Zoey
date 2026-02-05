# Zoey Skills Integration Guide

## What This Document Is

A critical assessment of every Zoey skill (plugin, function, workflow), how they expose credential surfaces, and how to integrate [Gap](https://github.com/mikekelly/gap) to eliminate the single worst security flaw in the current architecture: **credentials living inside the agent process**.

This is not aspirational. This is what you need to ship before any production deployment.

---

## The Problem Nobody Wants to Talk About

Zoey's plugins access external services by reading API keys from environment variables. The agent process holds these credentials in memory. A prompt injection attack - which is not theoretical, it is the most common attack vector against LLM agents - can exfiltrate every credential the agent can see.

Today, if you deploy Zoey with `RAG_GITHUB_TOKEN`, `OPENAI_API_KEY`, `INSTAGRAM_ACCESS_TOKEN`, and `X402_WALLET_ADDRESS` set as env vars, a single malicious prompt can ask the agent to echo, log, or transmit all of them. The agent has no mechanism to refuse because it has no concept of credential boundaries. It just sees strings.

This is not a Zoey-specific problem. It is the default state of every AI agent framework. But Zoey has the plugin architecture to fix it properly.

---

## Gap: What It Actually Does

[Gap](https://github.com/mikekelly/gap) is a localhost HTTPS proxy that intercepts outgoing HTTP requests and injects credentials at the network layer. The agent never sees the credential. Instead:

1. The agent gets a **Gap token** (a session-scoped, machine-local opaque string)
2. The agent makes HTTP requests through Gap's proxy (`https://localhost:9443`)
3. Gap matches the request hostname to a **plugin** (e.g., `api.github.com` -> GitHub plugin)
4. Gap injects the real credential (Bearer token, API key, etc.) into the request header
5. The request goes to the upstream service with real credentials; the response comes back through Gap to the agent

**What the agent never has access to:**
- The actual API key
- The actual OAuth token
- The actual wallet private key
- Any credential material whatsoever

**What Gap provides on top:**
- Auditable request logs (every API call the agent makes is recorded)
- Rate limiting at the credential level (not just the endpoint level)
- Credential rotation without agent restart
- Revocation of agent access without touching the credential itself

---

## Skill-by-Skill Assessment

### Skills That MUST Integrate Gap

These plugins make outbound HTTP requests to external services with credentials. They are the attack surface.

#### 1. zoey-plugin-rag-connectors

**Current state:** Reads `RAG_GITHUB_TOKEN` and similar tokens from env vars. Passes them directly in HTTP headers via `reqwest`.

**Risk level:** Critical. This plugin ingests data from GitHub, Notion, Google Drive, and arbitrary web URLs. It is the most likely vector for prompt injection because it processes untrusted external content that could contain injection payloads.

**Gap integration:**
- Configure Gap plugins for `api.github.com`, `api.notion.com`, `www.googleapis.com`
- Route all `reqwest` calls through `https://localhost:9443` by setting the HTTP proxy
- Remove `RAG_GITHUB_TOKEN` and similar env vars from the agent process entirely
- The `AddSourceAction`, `ForceRefreshAction`, and `SearchSourceAction` require zero code changes - only the HTTP client configuration changes

**Impact:** Eliminates the highest-risk credential exposure in the entire system.

#### 2. zoey-plugin-search

**Current state:** Makes HTTP requests to DuckDuckGo (currently no auth required) but the architecture supports swapping in authenticated search providers.

**Risk level:** Medium. Low today with DuckDuckGo, but becomes critical the moment someone configures a paid search provider (Exa, Serper, Tavily) that requires an API key.

**Gap integration:**
- Create Gap plugins for each search provider hostname
- Route search HTTP client through Gap proxy
- Future-proofs the search plugin for any authenticated provider

**Impact:** Prevents credential leakage when upgrading to premium search.

#### 3. zoey-plugin-knowledge

**Current state:** Optionally calls OpenAI API for embeddings when the `knowledge_real` feature flag is enabled. Reads `OPENAI_API_KEY` from env.

**Risk level:** High. OpenAI keys are high-value targets. A leaked key can generate unbounded costs.

**Gap integration:**
- Configure Gap plugin for `api.openai.com`
- Route embedding requests through Gap proxy
- Remove `OPENAI_API_KEY` from agent env

**Impact:** Protects the most expensive credential in a typical deployment.

#### 4. zoey-plugin-x402-video

**Current state:** Calls 5 video generation APIs (Sora, Replicate, Runway, Pika, Luma) and 3 social media APIs (Instagram, TikTok, Snapchat). Reads API keys and access tokens from env vars. Also handles cryptocurrency wallet addresses for X402 payment protocol.

**Risk level:** Critical. This plugin holds the most credentials of any single plugin (potentially 8+ API keys). It also handles financial transactions through X402. A compromised wallet address or social media token can cause direct financial and reputational damage.

**Gap integration:**
- Configure Gap plugins for each video provider API hostname
- Configure Gap plugins for `graph.facebook.com` (Instagram), TikTok Content API, Snapchat Marketing API
- X402 wallet operations need special handling: Gap should proxy the facilitator URL and inject payment credentials
- This plugin needs the most Gap plugins but benefits the most from the integration

**Impact:** Closes the widest credential attack surface in Zoey.

#### 5. LLM Providers (zoey-provider-anthropic, openai, redpill)

**Current state:** Each provider reads its API key from env vars (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `REDPILL_API_KEY`). These are the most frequently called external services.

**Risk level:** High. LLM API keys are called on every single user interaction. They are always in memory, always active.

**Gap integration:**
- Configure Gap plugins for `api.anthropic.com`, `api.openai.com`, and the Redpill endpoint
- Route all LLM HTTP clients through Gap
- This is the single highest-volume integration point

**Impact:** Protects the credentials used most frequently.

---

### Skills That Do NOT Need Gap

These plugins operate locally. They make no outbound HTTP requests with credentials.

| Plugin | Why No Gap Needed |
|--------|-------------------|
| **zoey-plugin-bootstrap** | Pure logic: actions, providers, evaluators. No external calls. |
| **zoey-plugin-hardware** | Reads local system info (CPU, GPU, RAM). No network. |
| **zoey-plugin-memory** | Reads/writes to local database. No external API. |
| **zoey-plugin-moderation** | Regex-based content detection. No external service. |
| **zoey-plugin-observability** | Local reasoning chains and audit logs. No external calls. |
| **zoey-plugin-lifeengine** | In-memory emotional/cognitive state machine. No network. |
| **zoey-plugin-scheduler** | Local task scheduling and cron. No external API. |
| **zoey-ext-workflow** | Local workflow orchestration engine. No external calls. |

---

## Implementation Guide

### Step 1: Install Gap on Your Deployment Host

```bash
# macOS
curl -fsSL https://github.com/mikekelly/gap/releases/latest/download/gap-mac.dmg -o gap.dmg
open gap.dmg

# Linux
curl -fsSL https://github.com/mikekelly/gap/releases/latest/download/gap-linux-x86_64 \
  -o /usr/local/bin/gap
chmod +x /usr/local/bin/gap
sudo gap install-service
```

### Step 2: Initialize Gap and Set a Passcode

```bash
gap init
```

This creates the encrypted credential store. On macOS, credentials go into Keychain. On Linux, they go into a directory owned by a dedicated service user that the agent process cannot read directly.

### Step 3: Install Gap Plugins for Each Service

Each external service Zoey talks to needs a Gap plugin. A plugin is a JavaScript file that tells Gap which hostname to match and how to inject credentials.

Example: GitHub plugin for rag-connectors:

```javascript
// github.gap.js
module.exports = {
  name: "github",
  hostname: "api.github.com",
  credentials: {
    token: { type: "string", description: "GitHub Personal Access Token" }
  },
  transform: (req, credentials) => {
    req.headers["Authorization"] = `Bearer ${credentials.token}`;
    return req;
  }
};
```

Install it:

```bash
gap plugin install ./github.gap.js
gap credentials set github token
# Paste your GitHub PAT when prompted - it is stored encrypted, write-only
```

Repeat for each service:
- `api.openai.com` (OpenAI - used by knowledge plugin and LLM provider)
- `api.anthropic.com` (Anthropic LLM provider)
- `api.exa.ai` or search provider of choice
- Video provider APIs (api.replicate.com, etc.)
- Social media APIs (graph.facebook.com, etc.)

### Step 4: Generate an Agent Token

```bash
gap token generate --name zoey-agent
```

This produces a token that Zoey will use to authenticate with Gap. The token is machine-local and session-scoped. It cannot be used from another machine.

### Step 5: Configure Zoey to Route Through Gap

Set the following environment variables for the Zoey agent process. Note: these replace the individual API key env vars.

```bash
# The only credential the agent process needs
export GAP_TOKEN="<token from step 4>"

# Tell reqwest to use Gap as HTTPS proxy
export HTTPS_PROXY="https://localhost:9443"

# Trust Gap's CA certificate
export SSL_CERT_FILE="/path/to/gap/ca.pem"

# REMOVE these - they are no longer needed:
# unset OPENAI_API_KEY
# unset ANTHROPIC_API_KEY
# unset RAG_GITHUB_TOKEN
# unset INSTAGRAM_ACCESS_TOKEN
# unset TIKTOK_ACCESS_TOKEN
# unset SNAPCHAT_ACCESS_TOKEN
# unset REPLICATE_API_KEY
# ... etc
```

### Step 6: Verify the Integration

```bash
# Check Gap is running
curl -k https://localhost:9443/health

# Check agent can reach services through Gap
gap logs --follow
# Then trigger a Zoey action that calls an external service
```

---

## What Changes in Zoey's Code

Minimal. That is the point.

The `reqwest` HTTP client respects standard proxy environment variables (`HTTPS_PROXY`). Every plugin that uses `reqwest::Client` to make outbound calls will automatically route through Gap when `HTTPS_PROXY` is set. No plugin code needs to change.

The only code change needed is **removing the env var reads for credentials** in plugins that currently inject them manually into request headers. If a plugin does:

```rust
let token = std::env::var("RAG_GITHUB_TOKEN")?;
client.get(url).header("Authorization", format!("Bearer {}", token))
```

It should become:

```rust
// Gap injects the Authorization header automatically
client.get(url)
```

The header injection happens at the proxy layer. The plugin just makes the request.

### Files That Need Changes

| File | Change |
|------|--------|
| `crates/plugins/zoey-plugin-rag-connectors/src/lib.rs` | Remove manual token injection from GitHub/Notion/Drive requests |
| `crates/plugins/zoey-plugin-knowledge/src/lib.rs` | Remove `OPENAI_API_KEY` read for embedding calls |
| `crates/plugins/zoey-plugin-search/src/lib.rs` | Remove manual API key injection (when using authenticated providers) |
| `crates/plugins/zoey-plugin-x402-video/src/lib.rs` | Remove manual token injection for video and social APIs |
| `crates/providers/zoey-provider-anthropic/src/lib.rs` | Remove `ANTHROPIC_API_KEY` header injection |
| `crates/providers/zoey-provider-openai/src/lib.rs` | Remove `OPENAI_API_KEY` header injection |
| `crates/providers/zoey-provider-redpill/src/lib.rs` | Remove `REDPILL_API_KEY` header injection |

---

## What This Does NOT Solve

Being critical means being honest about limitations:

1. **Local model access is unaffected.** `zoey-provider-local` (Ollama, llama.cpp) runs on localhost. Gap does not proxy localhost-to-localhost traffic. This is fine - there are no credentials to protect.

2. **Database credentials are separate.** Zoey's storage adapters (PostgreSQL, MongoDB, Supabase) use connection strings, not HTTP API keys. Gap proxies HTTP, not database wire protocols. Database credential management needs a different solution (vault, IAM roles, etc.).

3. **Gap does not prevent data exfiltration.** An agent can still be tricked into sending sensitive *data* (not credentials) to an attacker-controlled endpoint. Gap only protects credentials, not the data the agent processes. Content moderation (zoey-plugin-moderation) and output filtering are still necessary.

4. **Gap is a single point of failure.** If Gap goes down, all external API calls fail. This is a trade-off: security boundary vs. availability. In production, Gap should be monitored and restarted automatically (systemd handles this on Linux).

5. **Gap's plugin model is JavaScript.** Zoey is Rust. There is no type-safe bridge between Gap plugin definitions and Zoey's plugin registry. A mismatch between what Gap expects and what Zoey sends will fail silently or with opaque proxy errors. Integration testing is essential.

---

## Skill Inventory Reference

Complete list of Zoey skills and their external dependencies:

### Plugins

| Skill | Actions | Providers | Evaluators | External Services | Gap Required |
|-------|---------|-----------|------------|-------------------|--------------|
| bootstrap | 8 (reply, ignore, none, send, follow, unfollow, ask, summarize) | 11 (time, character, actions, entities, messages, context, dialogue, session, recall, reaction planner, output planner) | 6 (reflection, fact extraction, goal tracking, direct answer, brevity, conversation review) | None | No |
| hardware | 0 | 0 | 0 | None | No |
| knowledge | 0 | 0 | 0 | OpenAI (optional) | Yes (when using embeddings) |
| lifeengine | 0 | 3 (soul state, emotion, drive) | 3 (emotion, drive, soul reflection) | None | No |
| memory | 0 | 1 (context memories) | 2 (summarization, long-term extraction) | None | No |
| moderation | 0 | 0 | 1 (content safety) | None | No |
| observability | 0 | 0 | 1 (explainability) | None | No |
| rag-connectors | 3 (add source, force refresh, search) | 0 | 0 | GitHub, Notion, Google Drive, Web | Yes |
| scheduler | 4 (create task, list, complete, schedule reminder) | 0 | 0 | None | No |
| search | 2 (web search, cache status) | 0 | 0 | DuckDuckGo (or paid provider) | Yes (for paid providers) |
| x402-video | 3 (generate video, post video, generate and post) | 2 (video platforms, x402 payment) | 0 | Sora, Replicate, Runway, Pika, Luma, Instagram, TikTok, Snapchat, X402 | Yes |

### Extensions

| Skill | Key Capability | External Services | Gap Required |
|-------|---------------|-------------------|--------------|
| workflow | Pipeline orchestration, conditional logic, distributed execution, cron scheduling | None | No |

### Function Calling

The `FunctionRegistry` is the low-level mechanism that LLM providers use to invoke actions. It does not make external calls itself. Functions registered by plugins inherit the plugin's external dependencies. Gap integration happens at the plugin level, not the function registry level.

### LLM Providers

| Provider | External Service | Gap Required |
|----------|-----------------|--------------|
| anthropic | api.anthropic.com | Yes |
| openai | api.openai.com | Yes |
| redpill | Redpill API endpoint | Yes |
| local | localhost (Ollama, llama.cpp, LocalAI) | No |
| router | Routes to above providers | Inherits from target |
| voice | Voice synthesis service | Yes (if external) |

---

## Deployment Checklist

Before going to production with Gap:

- [ ] Gap installed and `gap init` completed
- [ ] Gap plugins created for every external service hostname
- [ ] Credentials set via `gap credentials set` (never via env vars)
- [ ] Agent token generated via `gap token generate`
- [ ] `HTTPS_PROXY=https://localhost:9443` set in agent env
- [ ] Gap CA certificate trusted by agent process
- [ ] All individual API key env vars removed from agent env
- [ ] Manual token/key injection removed from plugin HTTP requests
- [ ] Integration tests passing through Gap proxy
- [ ] Gap logs monitored (systemd journal or equivalent)
- [ ] Gap health check integrated into Zoey's `/health` endpoint
- [ ] Credential rotation procedure documented and tested

---

## Final Assessment

Zoey has 12 skills. 5 of them (plus 3 LLM providers) make authenticated external API calls. Today, every one of those credentials sits in the agent's process memory as a plaintext environment variable. This is the default, and it is indefensible for production.

Gap integration is not optional hardening. It is the minimum viable security posture for an AI agent that handles real credentials. The integration is low-friction because `reqwest` already supports proxy configuration, and Gap handles credential injection transparently.

The work is: write ~10 Gap plugins (JavaScript, ~20 lines each), remove ~30 lines of manual credential injection from Zoey's Rust code, and set two environment variables. The security gain is that no credential ever enters the agent's address space.

Do this before you deploy. Not after.
