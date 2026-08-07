FROM node:22-bookworm-slim AS build

WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY tsconfig.json ./
COPY src ./src
RUN npm run check && npm run build:node

FROM node:22-bookworm-slim AS runtime

ENV NODE_ENV=production \
    NODE_OPTIONS=--enable-source-maps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci --omit=dev && npm cache clean --force
COPY --from=build /app/dist-node ./dist-node
COPY migrations ./migrations
COPY scripts ./scripts

USER 1000:101
EXPOSE 8787
CMD ["sh", "-c", "node scripts/migrate.mjs && exec node dist-node/server.mjs"]
