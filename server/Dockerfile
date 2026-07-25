# syntax=docker/dockerfile:1
FROM node:25-bookworm-slim AS build
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 build-essential ca-certificates \
  && rm -rf /var/lib/apt/lists/*
COPY package.json package-lock.json* ./
RUN npm ci
COPY tsconfig.json ./
COPY src ./src
RUN npm run build
RUN npm prune --omit=dev

FROM node:25-bookworm-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production
ENV DB_PATH=/var/lib/sharepaste/sharepaste.sqlite
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/* \
  && mkdir -p /var/lib/sharepaste \
  && chown -R node:node /var/lib/sharepaste
USER node
COPY --from=build --chown=node:node /app/node_modules ./node_modules
COPY --from=build --chown=node:node /app/dist ./dist
COPY --from=build --chown=node:node /app/package.json ./package.json
VOLUME /var/lib/sharepaste
EXPOSE 8443
ENTRYPOINT ["node", "dist/index.js"]
CMD ["serve"]
