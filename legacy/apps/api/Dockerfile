FROM node:22-alpine AS base

WORKDIR /app

RUN npm install -g pnpm

# Copy monorepo config files
COPY package.json pnpm-workspace.yaml turbo.json tsconfig.json ./

# Copy packages and app definition
COPY packages/shared ./packages/shared
COPY packages/db ./packages/db
COPY apps/api ./apps/api

# Install dependencies
RUN pnpm install

# Build shared and db packages first since apps/api depends on them
RUN pnpm --filter @akasha/shared build
RUN pnpm --filter @akasha/db build

# Harden process context: Switch to non-root node user
RUN chown -R node:node /app
USER node

EXPOSE 3001

CMD ["pnpm", "--filter", "@akasha/api", "dev"]
