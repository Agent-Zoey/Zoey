<p align="center">
  <img src="../../../assets/zoey-confident.png" alt="Zoey" width="200" />
</p>

# 🧪 Supabase Storage Integration Tests

> **Your secrets are safe with Zoey**

Integration tests for Zoey's Supabase database adapter.

---

## Quick Start

### Supabase Tests (Setup Required)

```bash
# Set environment variables
export SUPABASE_URL="https://your-project.supabase.co"
export SUPABASE_ANON_KEY="your-anon-key"

# Run tests
cargo test -p zoey-storage-supabase --test supabase_integration_tests -- --ignored
```

---

## Database Setup

### Required Tables

Run these SQL migrations in your Supabase SQL Editor:

```sql
-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Enable pgvector for embeddings
CREATE EXTENSION IF NOT EXISTS vector;

-- Agents table
CREATE TABLE IF NOT EXISTS agents (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    character JSONB NOT NULL,
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
    updated_at BIGINT
);

-- Entities table
CREATE TABLE IF NOT EXISTS entities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    name TEXT,
    username TEXT,
    email TEXT,
    avatar_url TEXT,
    metadata JSONB DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
);

-- Worlds table
CREATE TABLE IF NOT EXISTS worlds (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name TEXT NOT NULL,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    server_id TEXT,
    metadata JSONB DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
);

-- Rooms table
CREATE TABLE IF NOT EXISTS rooms (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID REFERENCES agents(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    source TEXT NOT NULL,
    type TEXT NOT NULL,
    channel_id TEXT,
    server_id TEXT,
    world_id UUID NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    metadata JSONB DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
);

-- Memories table with vector support
CREATE TABLE IF NOT EXISTS memories (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    entity_id UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    content JSONB NOT NULL,
    embedding vector(1536),
    metadata JSONB,
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
    unique_flag BOOLEAN DEFAULT FALSE
);

-- Participants table
CREATE TABLE IF NOT EXISTS participants (
    entity_id UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    room_id UUID NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    joined_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
    metadata JSONB DEFAULT '{}',
    PRIMARY KEY (entity_id, room_id)
);

-- Relationships table
CREATE TABLE IF NOT EXISTS relationships (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    entity_id_a UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    entity_id_b UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    type TEXT NOT NULL,
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    metadata JSONB DEFAULT '{}',
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
    UNIQUE (entity_id_a, entity_id_b, type)
);

-- Components table
CREATE TABLE IF NOT EXISTS components (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    entity_id UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    world_id UUID NOT NULL REFERENCES worlds(id) ON DELETE CASCADE,
    source_entity_id UUID REFERENCES entities(id) ON DELETE SET NULL,
    type TEXT NOT NULL,
    data JSONB NOT NULL,
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
    updated_at BIGINT
);

-- Tasks table
CREATE TABLE IF NOT EXISTS tasks (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    agent_id UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    task_type TEXT NOT NULL,
    data JSONB NOT NULL,
    status TEXT NOT NULL DEFAULT 'PENDING',
    priority INTEGER DEFAULT 0,
    scheduled_at BIGINT,
    executed_at BIGINT,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 3,
    error TEXT,
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint,
    updated_at BIGINT
);

-- Logs table
CREATE TABLE IF NOT EXISTS logs (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    entity_id UUID NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    room_id UUID REFERENCES rooms(id) ON DELETE SET NULL,
    body JSONB NOT NULL,
    type TEXT NOT NULL,
    created_at BIGINT NOT NULL DEFAULT extract(epoch from now())::bigint
);

-- LLM Costs table
CREATE TABLE IF NOT EXISTS llm_costs (
    id UUID PRIMARY KEY,
    timestamp BIGINT NOT NULL,
    agent_id UUID NOT NULL,
    user_id TEXT,
    conversation_id UUID,
    action_name TEXT,
    evaluator_name TEXT,
    provider TEXT NOT NULL,
    model TEXT NOT NULL,
    temperature REAL NOT NULL,
    prompt_tokens BIGINT NOT NULL,
    completion_tokens BIGINT NOT NULL,
    total_tokens BIGINT NOT NULL,
    cached_tokens BIGINT,
    input_cost_usd REAL NOT NULL,
    output_cost_usd REAL NOT NULL,
    total_cost_usd REAL NOT NULL,
    latency_ms BIGINT NOT NULL,
    ttft_ms BIGINT,
    success BOOLEAN NOT NULL,
    error TEXT,
    prompt_hash TEXT,
    prompt_preview TEXT
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_memories_agent_id ON memories(agent_id);
CREATE INDEX IF NOT EXISTS idx_memories_room_id ON memories(room_id);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at DESC);

-- Create HNSW index for vector search
CREATE INDEX IF NOT EXISTS idx_memories_embedding ON memories 
USING hnsw (embedding vector_cosine_ops);
```

### Vector Search Function

```sql
CREATE OR REPLACE FUNCTION match_memories(
  query_embedding vector(1536),
  match_count int DEFAULT 10,
  filter_agent_id uuid DEFAULT NULL,
  filter_room_id uuid DEFAULT NULL,
  similarity_threshold float DEFAULT 0.7
)
RETURNS TABLE (
  id uuid,
  entity_id uuid,
  agent_id uuid,
  room_id uuid,
  content jsonb,
  metadata jsonb,
  created_at bigint,
  unique_flag boolean,
  similarity float
)
LANGUAGE plpgsql
AS $$
BEGIN
  RETURN QUERY
  SELECT
    m.id,
    m.entity_id,
    m.agent_id,
    m.room_id,
    m.content,
    m.metadata,
    m.created_at,
    m.unique_flag,
    1 - (m.embedding <=> query_embedding) as similarity
  FROM memories m
  WHERE
    m.embedding IS NOT NULL
    AND (filter_agent_id IS NULL OR m.agent_id = filter_agent_id)
    AND (filter_room_id IS NULL OR m.room_id = filter_room_id)
    AND 1 - (m.embedding <=> query_embedding) > similarity_threshold
  ORDER BY m.embedding <=> query_embedding
  LIMIT match_count;
END;
$$;
```

---

## Test Coverage

### Supabase Tests ✅
- Agent CRUD operations
- Entity CRUD operations
- Memory CRUD operations
- Memory filtering (room, unique flag, limits)
- World and Room management
- Task scheduling
- Participant management
- Vector search (requires pgvector setup)

---

## Running All Tests

```bash
# Supabase integration tests
cargo test -p zoey-storage-supabase --tests -- --ignored
```

---

## Troubleshooting

### Connection Refused

```bash
# Verify Supabase URL and key
echo $SUPABASE_URL
echo $SUPABASE_ANON_KEY
```

### Permission Denied

Make sure you're using the correct API key:
- Use `anon` key for client-side operations
- Use `service_role` key for admin operations (bypass RLS)

### Vector Search Not Working

1. Ensure pgvector extension is enabled
2. Ensure the `match_memories` function exists
3. Check that memories have embeddings stored

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
