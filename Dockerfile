# Stage 1: Build the Wasm module
FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev curl binaryen

RUN curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

WORKDIR /app

COPY raw2dng/Cargo.toml raw2dng/Cargo.lock ./

RUN mkdir src

COPY raw2dng/src ./src
RUN wasm-pack build --target web --release

FROM docker.angie.software/angie:minimal

WORKDIR /usr/share/angie/html

COPY raw2dng/www/ ./

COPY --from=builder /app/pkg/ ./pkg/

COPY angie/angie.conf /etc/angie/http.d/default.conf

EXPOSE 80

CMD ["angie", "-g", "daemon off;"]
