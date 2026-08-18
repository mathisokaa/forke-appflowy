## 🎯 Overview

AppFlowy Web requires AppFlowy Cloud as its backend. You can set up this pair in two ways:

- **🛠️ Development Mode** (`dev.env`) - For local development and testing
- **🚀 Production Mode** (`deploy.env`) - For production deployments with Docker

## 📋 Prerequisites

Before you begin, ensure you have:

- **Node.js** ≥18.0.0
- **pnpm** ≥10.9.0  
- **Docker & Docker Compose** (required for both modes)

## 🛠️ Development Mode Setup

**Best for:** Local development, testing, and debugging individual services.

### Step-by-Step Setup

#### 1. Set Up AppFlowy Cloud (Development)

> 💡 **Tip**: The `generate_env.sh` script creates a proper `.env` file with all necessary configurations. Check the [AppFlowy Cloud README](https://github.com/AppFlowy-IO/AppFlowy-Cloud/blob/main/README.md) for more details.
```bash
# Clone AppFlowy Cloud repository
git clone https://github.com/AppFlowy-IO/AppFlowy-Cloud.git
cd AppFlowy-Cloud

# Use development configuration
# The `generate_env.sh` script creates a proper `.env` file with all necessary configurations. 
./script/generate_env.sh 

# Start development server
# For new setup - RECOMMENDED FOR FIRST TIME
./script/run_local_server.sh --reset

# Or run (interactive prompts for container management)
./script/run_local_server.sh
```

#### 2. Set Up AppFlowy Web (Development)

```bash
# In a new terminal, navigate to your AppFlowy Web directory
cd /path/to/appflowy-web
cp dev.env .env

# Install dependencies and start
corepack enable
pnpm install
pnpm run dev
```


## 🚀 Production Mode Setup

**Best for:** Production deployments, staging environments, and containerized setups.


#### 1. Set Up AppFlowy Cloud (Production)

```bash
# Clone AppFlowy Cloud repository
git clone https://github.com/AppFlowy-IO/AppFlowy-Cloud.git
cd AppFlowy-Cloud

# Use production configuration
# The `generate_env.sh` script creates a proper `.env` file with all necessary configurations. 
./script/generate_env.sh 

# Start with Docker Compose
docker compose up -d
```

#### 2. Set Up AppFlowy Web (Production)

```bash
# In a new terminal, navigate to your AppFlowy Web directory
cd /path/to/appflowy-web

# Use matching production configuration
cp deploy.env .env

# Install dependencies and start
corepack enable
pnpm install
pnpm run dev
```

## 🔗 Additional Resources

- **[AppFlowy Cloud Repository](https://github.com/AppFlowy-IO/AppFlowy-Cloud)** - Backend setup and configuration
- **[AppFlowy Web README](../README.md)** - Frontend development guide  
- **[AppFlowy Documentation](https://appflowy.com/docs)** - Official product documentation
- **[AppFlowy GitHub Discussions](https://github.com/AppFlowy-IO/AppFlowy/discussions)** - Community support
