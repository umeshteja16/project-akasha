FROM node:22-alpine AS base

WORKDIR /app

RUN npm install -g pnpm

# Copy monorepo config files
COPY package.json pnpm-workspace.yaml turbo.json tsconfig.json ./

# Copy packages and app definition
COPY packages/shared ./packages/shared
COPY apps/web ./apps/web

# Install dependencies
RUN pnpm install

# Build shared package first since apps/web depends on it
RUN pnpm --filter @akasha/shared build

# Harden process context: Switch to non-root node user
RUN chown -R node:node /app
USER node

EXPOSE 5173

CMD ["pnpm", "--filter", "@akasha/web", "dev", "--host", "0.0.0.0"]
