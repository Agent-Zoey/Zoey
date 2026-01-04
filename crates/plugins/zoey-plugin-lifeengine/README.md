# zoey-plugin-lifeengine

**AI Souls with Personality, Emotion, Drive, and Mental Processes**

Inspired by [OpenSouls Soul Engine](https://github.com/opensouls/opensouls), this plugin brings rich psychological modeling to ZoeyAI agents.

## Overview

The Life Engine plugin transforms Zoey agents from simple chatbots into AI beings with:

- **Personality** - Big Five traits (OCEAN model) that influence response style
- **Emotion** - Dynamic emotional state using PAD model and Plutchik's wheel
- **Drive** - Motivational system with needs like connection, helpfulness, curiosity
- **Mental Processes** - State machine for behavioral modes that automatically adapt to context
- **Working Memory** - Cognitive workspace for active reasoning and thought

## Key Features

### 🧠 Working Memory

Unlike conversation history, working memory captures the agent's internal thought process:

```rust
use zoey_plugin_lifeengine::{WorkingMemory, ThoughtFragment, ThoughtType};

let memory = WorkingMemory::new()
    .push(ThoughtFragment::new(
        "User seems frustrated about the error",
        ThoughtType::Perception,
        ThoughtSource::External { entity_id: None, channel: "chat".into() }
    ))
    .push(ThoughtFragment::new(
        "I should respond with empathy first",
        ThoughtType::Intention,
        ThoughtSource::Internal { process: "emotional_intelligence".into() }
    ));

// Thoughts have salience, TTL, and can be queried by type
let emotions = memory.thoughts_by_type(ThoughtType::Emotion);
let most_important = memory.most_salient(5);
```

### 💭 Mental Processes

Behavioral modes that automatically transition based on context:

```rust
use zoey_plugin_lifeengine::mental_process::library;

// Built-in processes
let introduction = library::introduction_process();     // First interactions
let active_listening = library::active_listening_process(); // Deep conversations
let problem_solving = library::problem_solving_process();   // Helping with problems
let emotional_support = library::emotional_support_process(); // Emotional comfort

// Each process modifies behavior
// - Style (verbosity, formality, empathy)
// - Goals (what to prioritize)
// - Actions (what's allowed/blocked)
```

### ❤️ Emotional State

Rich emotional modeling with PAD dimensions and discrete emotions:

```rust
use zoey_plugin_lifeengine::{EmotionalState, DiscreteEmotion};

let mut state = EmotionalState::with_baseline(0.2, 0.5, 0.6); // Slightly positive

// Process emotional events
state.process_event("user_gratitude", DiscreteEmotion::Joy, 0.8);

// Get emotional context
println!("{}", state.describe());
// "Currently moderately joyful (feeling pleasure and happiness)"

// Emotions naturally decay over time
state.decay();
```

### 🎯 Drive System

Motivational needs that influence behavior:

```rust
use zoey_plugin_lifeengine::{Drive, soul_config::drives};

// Built-in drives
let connection = drives::connection();    // Need for meaningful connection
let helpfulness = drives::helpfulness();  // Need to assist and provide value
let curiosity = drives::curiosity();      // Need to learn and understand
let accuracy = drives::accuracy();        // Need for truth and correctness
let autonomy = drives::autonomy();        // Need for self-expression

// Drives are satisfied or frustrated by interactions
// High drives influence the agent's priorities
```

### 🎭 Personality

Big Five personality traits that shape response style:

```rust
use zoey_plugin_lifeengine::PersonalityTraits;

// Preset personalities
let supportive = PersonalityTraits::supportive();
let creative = PersonalityTraits::creative();
let analytical = PersonalityTraits::analytical();

// Or customize
let custom = PersonalityTraits {
    openness: 0.8,          // Creative, curious
    conscientiousness: 0.7, // Organized, reliable
    extraversion: 0.6,      // Outgoing but not overwhelming
    agreeableness: 0.9,     // Warm, cooperative
    neuroticism: 0.3,       // Emotionally stable
};
```

### 🔮 Soul Configuration

Complete soul definition:

```rust
use zoey_plugin_lifeengine::{
    SoulConfig, PersonalityTraits, Ego, VoiceStyle,
    StaticMemory, soul_config::drives
};

let samantha = SoulConfig::new("Samantha")
    .with_personality(PersonalityTraits::supportive())
    .with_drive(drives::connection())
    .with_drive(drives::curiosity())
    .with_ego(Ego::with_identity("A warm, curious companion"))
    .with_voice(VoiceStyle::warm())
    .with_static_memory(
        StaticMemory::new("backstory", "Created to understand and connect with humans")
    );
```

## Integration

### As a Plugin

```rust
use zoey_plugin_lifeengine::{LifeEnginePlugin, LifeEngineConfig, supportive_soul};

// Default configuration
let plugin = LifeEnginePlugin::new();

// Or with custom soul
let config = LifeEngineConfig::default()
    .with_soul(supportive_soul("Zoey"));
let plugin = LifeEnginePlugin::with_config(config);

// Register with runtime
runtime.register_plugin(Arc::new(plugin)).await?;
```

### Providers

The plugin provides three context providers for LLM prompts:

1. **`soul_state`** - Complete soul context (personality, identity, mode)
2. **`emotion`** - Current emotional state and mood
3. **`drives`** - Active drives and motivations

### Evaluators

Post-response evaluators update soul state:

1. **`emotion_update`** - Updates emotions based on conversation
2. **`drive_update`** - Updates drives based on satisfiers/frustrators
3. **`soul_reflection`** - Periodic self-reflection on conversation quality

### Service

The `SoulEngineService` manages:
- Per-entity soul states
- Working memory lifecycle
- Mental process orchestration
- Emotional decay and drive updates

```rust
// Access via runtime
if let Some(service) = runtime.get_service("soul_engine") {
    if let Some(engine) = service.as_any().downcast_ref::<SoulEngineService>() {
        // Get or create state for an entity
        let state = engine.get_or_create_state(entity_id, room_id);
        
        // Process a message
        engine.process_message(&message, &mut state).await?;
        
        // Generate LLM context
        let context = engine.generate_context(&state);
    }
}
```

## Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `LIFEENGINE_SESSION_TIMEOUT` | 3600 | Session idle timeout (seconds) |
| `LIFEENGINE_MAX_SESSIONS` | 1000 | Maximum concurrent sessions |
| `LIFEENGINE_PERSIST_STATE` | true | Persist state between sessions |
| `LIFEENGINE_EMOTION_DECAY_INTERVAL` | 60 | Emotion decay interval (seconds) |
| `LIFEENGINE_DRIVE_UPDATE_INTERVAL` | 300 | Drive update interval (seconds) |
| `LIFEENGINE_DEFAULT_PERSONALITY` | supportive | Default personality preset |
| `LIFEENGINE_WORKING_MEMORY_SIZE` | 50 | Max thoughts in working memory |

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Life Engine Plugin                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐   ┌──────────────┐   ┌──────────────┐        │
│  │  Providers   │   │  Evaluators  │   │   Service    │        │
│  ├──────────────┤   ├──────────────┤   ├──────────────┤        │
│  │ SoulState    │   │ Emotion      │   │ SoulEngine   │        │
│  │ Emotion      │   │ Drive        │   │              │        │
│  │ Drive        │   │ Reflection   │   │              │        │
│  └──────────────┘   └──────────────┘   └──────────────┘        │
│         │                  │                  │                 │
│         └──────────────────┼──────────────────┘                 │
│                            │                                    │
│  ┌─────────────────────────┴─────────────────────────┐         │
│  │                    Soul State                      │         │
│  ├────────────────────────────────────────────────────┤         │
│  │  ┌──────────────┐  ┌──────────────┐  ┌──────────┐ │         │
│  │  │   Working    │  │  Emotional   │  │  Mental  │ │         │
│  │  │   Memory     │  │    State     │  │ Process  │ │         │
│  │  └──────────────┘  └──────────────┘  └──────────┘ │         │
│  └────────────────────────────────────────────────────┘         │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                     Core Types                              ││
│  ├─────────────────────────────────────────────────────────────┤│
│  │  ThoughtFragment  CognitiveStep  DiscreteEmotion  Drive     ││
│  │  CoreAffect       MentalProcess  PersonalityTraits  Ego     ││
│  │  SoulConfig       VoiceStyle     StaticMemory              ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

## Benefits

1. **More Human-Like Interactions** - Emotional intelligence and personality create natural conversations
2. **Context-Aware Behavior** - Mental processes automatically adapt to conversation type
3. **Memory Beyond History** - Working memory captures reasoning, not just messages
4. **Motivation-Driven** - Drives create consistent character motivations
5. **Debuggable Cognition** - Immutable working memory makes thought processes traceable

## Inspired By

This plugin draws inspiration from:

- [OpenSouls Soul Engine](https://github.com/opensouls/opensouls) - The framework for AI souls
- PAD Emotional Model - Pleasure-Arousal-Dominance dimensional model
- Plutchik's Wheel of Emotions - Discrete emotion taxonomy
- Big Five Personality Traits - OCEAN model of personality

## License

MIT License - See [LICENSE](../../../LICENSE) for details.

