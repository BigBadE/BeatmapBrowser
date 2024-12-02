FROM rust:latest AS builder

WORKDIR /app
COPY . .

RUN rm -rf site
RUN rm -rf oneclick

RUN git clone https://github.com/beatblock-browser/BeatblockBrowser.git site
RUN git clone https://github.com/BigBadE/Beatblock-Oneclick.git oneclick

RUN cargo build --release --bin backend

FROM debian:bookworm-slim AS bullseye
RUN apt-get update && apt-get install -y nginx curl bash libssl-dev dos2unix && apt-get clean && rm -rf /var/lib/apt/lists/*

COPY config/nginx.conf /etc/nginx/nginx.conf

COPY --from=builder /app/site /usr/local/site
COPY --from=builder /app/target/release/backend /usr/local/

# Copy builder data
COPY --from=builder /app/scripts/ /usr/local/
COPY --from=builder /app/config/ /usr/local/config/

RUN dos2unix "/usr/local/setup.sh"
RUN chmod +x "/usr/local/setup.sh"

ENTRYPOINT ["/usr/local/setup.sh"]