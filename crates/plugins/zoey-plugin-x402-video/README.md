<p align="center">
  <img src="../../assets/zoey-happy.png" alt="Zoey" width="250" />
</p>

# 🎬 zoey-plugin-x402-video

> **Your secrets are safe with Zoey**

X402 payment-gated AI video generation with multi-platform social media posting. Accept cryptocurrency payments for video generation and automatically distribute to Instagram, TikTok, and Snapchat.

## Status: ✅ Production

---

## Features

### 💳 X402 HTTP Payment Protocol
- Accept cryptocurrency payments (USDC on Base)
- Automatic payment verification via facilitator
- Transaction tracking on x402scan.com
- Configurable pricing

### 🎥 AI Video Generation
Support for multiple providers:

| Provider | Description |
|----------|-------------|
| **Replicate** | Stable Video Diffusion, AnimateDiff |
| **Runway ML** | Gen-2 professional video |
| **Pika Labs** | High-quality text-to-video |
| **Luma AI** | Dream Machine cinematic |
| **OpenAI Sora** | State-of-the-art generation |

### 📱 Multi-Platform Posting
| Platform | Features |
|----------|----------|
| **Instagram** | Reels & Videos, up to 90s |
| **TikTok** | Public/Private, up to 3 min |
| **Snapchat** | Stories & Spotlight, up to 60s |

---

## Quick Start

```rust
use zoey_plugin_x402_video::X402VideoPlugin;

// Load from environment
let plugin = X402VideoPlugin::from_env();

// Get the axum Router
let router = plugin.build_router();

// Merge with your app
let app = axum::Router::new()
    .merge(router);
```

---

## Configuration

### Environment Variables

```bash
# X402 Payment Configuration
X402_WALLET_ADDRESS=0x...          # Your wallet for settlement
X402_PRICE_CENTS=100               # Price in cents (default: $1.00)
X402_FACILITATOR_URL=https://facilitator.payai.network

# Video Generation
VIDEO_PROVIDER=replicate           # replicate, runway, pika, luma, sora
REPLICATE_API_KEY=...              # API key for your provider

# Instagram
INSTAGRAM_ACCESS_TOKEN=...
INSTAGRAM_BUSINESS_ACCOUNT_ID=...

# TikTok
TIKTOK_ACCESS_TOKEN=...
TIKTOK_CREATOR_ID=...

# Snapchat
SNAPCHAT_ACCESS_TOKEN=...
SNAPCHAT_ORGANIZATION_ID=...
```

### Programmatic Configuration

```rust
use zoey_plugin_x402_video::{
    X402VideoPlugin, X402VideoConfig, X402Config, 
    VideoGenerationConfig, VideoProvider, PlatformConfigs,
};

let config = X402VideoConfig {
    x402: X402Config {
        wallet_address: "0x...".to_string(),
        default_price_cents: 100,
        supported_networks: vec!["base".to_string()],
        supported_tokens: vec!["USDC".to_string()],
        ..Default::default()
    },
    video_generation: VideoGenerationConfig {
        provider: VideoProvider::Replicate,
        api_key_env: "REPLICATE_API_KEY".to_string(),
        ..Default::default()
    },
    platforms: PlatformConfigs {
        instagram: Some(InstagramConfig { enabled: true, ..Default::default() }),
        tiktok: None,
        snapchat: None,
    },
};

let plugin = X402VideoPlugin::new(config);
```

---

## Actions

### GENERATE_VIDEO

Generate an AI video from a text prompt. Requires x402 payment.

```
User: Generate video of a sunset over the ocean with dolphins jumping
```

The action will:
1. Check for x402 payment proof
2. If no payment, return HTTP 402 with payment requirements
3. If payment verified, generate the video
4. Return the video URL

### POST_VIDEO

Post a generated video to social media platforms.

```
User: Post that video to Instagram and TikTok with caption "Amazing sunset! 🌅"
```

### GENERATE_AND_POST_VIDEO

Combined action for generate and post in one step.

```
User: Generate a video of dancing robots and post it to all platforms
```

---

## REST API Endpoints

### GET /x402-video/pricing

Get pricing and configuration info.

```json
{
  "base_price_cents": 100,
  "networks": ["base"],
  "tokens": ["USDC"],
  "video_provider": "Replicate",
  "enabled_platforms": ["instagram", "tiktok"]
}
```

### POST /x402-video/generate

Generate a video (requires x402 payment).

**Request:**
```json
{
  "prompt": "A cat playing piano in a jazz club",
  "platforms": ["instagram", "tiktok"],
  "options": {
    "duration_secs": 4,
    "resolution": "HD720p"
  }
}
```

**Without payment (402 response):**
```json
{
  "error": "Payment required",
  "payment": {
    "scheme": "x402",
    "network": "base",
    "asset": "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913",
    "amount": "1000000",
    "pay_to": "0x..."
  }
}
```

**With valid X-402 header:**
```json
{
  "job_id": "gen-12345",
  "status": "Processing",
  "estimated_time_secs": 120
}
```

### GET /x402-video/status/:job_id

Check video generation status.

```json
{
  "job_id": "gen-12345",
  "status": "Completed",
  "video_url": "https://...",
  "platform_results": [
    {"platform": "instagram", "success": true, "post_url": "..."}
  ]
}
```

---

## X402 Payment Flow

```
┌──────────┐    1. Request Video    ┌──────────┐
│  Client  │ ─────────────────────▶ │  Zoey    │
│          │                        │          │
│          │ ◀───────────────────── │          │
│          │    2. HTTP 402 +       │          │
│          │       Payment Info     │          │
│          │                        │          │
│          │    3. X-402 Header     │          │
│          │ ─────────────────────▶ │          │
│          │       (with proof)     │          │
│          │                        │          │
│          │ ◀───────────────────── │          │
│          │    4. Video Generated  │          │
└──────────┘                        └──────────┘
                    │
                    │ Verify
                    ▼
            ┌──────────────┐
            │  Facilitator │
            │   (PayAI)    │
            └──────────────┘
```

### x402scan Transaction Tracking

Payments routed through PayAI facilitator are tracked on [x402scan.com](https://x402scan.com):

```bash
# Your wallet receives settlement from PayAI
X402_WALLET_ADDRESS=0xYourWallet

# PayAI facilitator address (tracked by x402scan)
X402_FACILITATOR_PAY_TO_ADDRESS=0xc6699d2aada6c36dfea5c248dd70f9cb0235cb63
```

---

## Platform-Specific Notes

### Instagram
- Requires Business Account connected to Facebook
- Maximum 90 seconds for Reels
- Supported aspect ratios: 9:16, 1:1, 4:5

### TikTok
- Requires TikTok for Developers account
- Maximum 3 minutes
- Supports privacy levels: Public, Followers, Private

### Snapchat
- Requires Marketing API access
- Maximum 60 seconds for Spotlight
- Only 9:16 aspect ratio supported

---

## Dependencies

- `zoey-core` - Core runtime and types

---

## Testing

```bash
cargo test -p zoey-plugin-x402-video
```

---

## License

MIT License

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
