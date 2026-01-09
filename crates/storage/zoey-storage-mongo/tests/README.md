<p align="center">
  <img src="../../../assets/zoey-confident.png" alt="Zoey" width="200" />
</p>

# 🧪 MongoDB Storage Integration Tests

> **Your secrets are safe with Zoey**

Integration tests for Zoey's MongoDB database adapter.

---

## Quick Start

### Install MongoDB Locally

#### Ubuntu/Debian

```bash
# Import MongoDB GPG key
curl -fsSL https://www.mongodb.org/static/pgp/server-7.0.asc | \
   sudo gpg -o /usr/share/keyrings/mongodb-server-7.0.gpg --dearmor

# Add MongoDB repository
echo "deb [ signed-by=/usr/share/keyrings/mongodb-server-7.0.gpg ] https://repo.mongodb.org/apt/ubuntu jammy/mongodb-org/7.0 multiverse" | \
   sudo tee /etc/apt/sources.list.d/mongodb-org-7.0.list

# Install MongoDB
sudo apt-get update
sudo apt-get install -y mongodb-org

# Start MongoDB service
sudo systemctl start mongod
sudo systemctl enable mongod

# Verify MongoDB is running
mongosh --eval "db.runCommand({ ping: 1 })"
```

#### macOS (Homebrew)

```bash
# Tap MongoDB formula
brew tap mongodb/brew

# Install MongoDB Community Edition
brew install mongodb-community@7.0

# Start MongoDB service
brew services start mongodb-community@7.0

# Verify MongoDB is running
mongosh --eval "db.runCommand({ ping: 1 })"
```

#### Arch Linux

```bash
# Install MongoDB from AUR
yay -S mongodb-bin

# Start MongoDB service
sudo systemctl start mongodb
sudo systemctl enable mongodb
```

---

### Run Tests

```bash
# Set MongoDB URL (localhost, no auth for local dev)
export MONGODB_URL="mongodb://localhost:27017"

# Run integration tests
cargo test -p zoey-storage-mongo --test mongo_integration_tests -- --ignored
```

---

## Using MongoDB Storage in Your Agent

### Character XML Configuration

```xml
<storage>
    <adapter>mongo</adapter>
    <url>${MONGODB_URL}</url>
    <database>zoey</database>
</storage>

<plugins>
    <plugin>zoey-storage-database</plugin>
    <!-- other plugins -->
</plugins>
```

### Environment Variables

```bash
# For local development (no auth)
export MONGODB_URL="mongodb://localhost:27017"

# For production (with auth)
export MONGODB_URL="mongodb://user:password@host:27017"
```

---

## Test Coverage

### MongoDB Tests ✅
- Agent CRUD operations
- Entity CRUD operations
- Memory CRUD operations
- Memory filtering (room, unique flag, limits)
- World and Room management
- Task scheduling
- Participant management

### Vector Search (Local)
- Uses aggregation pipeline with cosine similarity calculation
- No Atlas Search required - works with any MongoDB instance
- Indexes created automatically on `embedding` field

---

## Running All Tests

```bash
# MongoDB integration tests
cargo test -p zoey-storage-mongo --tests -- --ignored
```

---

## Troubleshooting

### Connection Refused

```bash
# Check MongoDB is running
sudo systemctl status mongod

# Or check the process
pgrep -l mongod
```

### Start MongoDB Manually

```bash
# Create data directory if needed
sudo mkdir -p /var/lib/mongodb
sudo chown mongodb:mongodb /var/lib/mongodb

# Start MongoDB
sudo systemctl start mongod
```

### Check MongoDB Logs

```bash
# View logs
sudo journalctl -u mongod -f

# Or check log file
tail -f /var/log/mongodb/mongod.log
```

### Reset MongoDB (Dev Only)

```bash
# Stop service
sudo systemctl stop mongod

# Remove data (WARNING: deletes all data!)
sudo rm -rf /var/lib/mongodb/*

# Restart
sudo systemctl start mongod
```

---

## MongoDB Shell Commands

```bash
# Connect to MongoDB
mongosh

# List databases
show dbs

# Use zoey database
use zoey

# List collections
show collections

# Query memories
db.memories.find().limit(5).pretty()

# Count documents
db.memories.countDocuments()
```

---

<p align="center">
  <strong>🔐 Your secrets are safe with Zoey</strong>
</p>
