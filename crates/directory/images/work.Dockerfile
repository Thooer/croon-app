# 文件执行的任务镜像：Directory 要求 /bin/sh、cp、sleep；Library 的 diff 与合并要 git。
# 建：docker build -t croon-work:1 -f crates/directory/images/work.Dockerfile crates/directory/images
# 官方源慢时换源，如：--build-arg ALPINE_MIRROR=https://mirrors.aliyun.com/alpine
FROM alpine:latest
ARG ALPINE_MIRROR=""
RUN if [ -n "$ALPINE_MIRROR" ]; then sed -i "s|https\?://dl-cdn.alpinelinux.org/alpine|$ALPINE_MIRROR|" /etc/apk/repositories; fi \
    && apk add --no-cache git
