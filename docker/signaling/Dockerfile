# Signaling Server Dockerfile
# Cloudflare Worker signaling service with Wrangler

FROM node:20-alpine

WORKDIR /app

# Install wrangler globally
RUN npm install -g wrangler

# Copy package files and install dependencies
COPY package*.json ./
RUN npm install

# Copy source code
COPY . .

# Expose Wrangler dev server port
EXPOSE 8787

# Start Wrangler dev server
CMD ["npx", "wrangler", "dev", "--local", "--port", "8787"]
