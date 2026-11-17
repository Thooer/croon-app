# 文件执行的任务镜像：Directory 要求 /bin/sh、cp、sleep；Library 的 diff 与合并要 git；
# 主体在里面写并运行程序，所以带 python3 与 pytest；模型习惯先用 rg 找文件，所以带 ripgrep。
# 容器没有网络，主体只能用标准库与这里装好的包。
# 建：docker build -t croon-work:2 -f crates/directory/images/work.Dockerfile crates/directory/images
# 官方源慢时换源，如：--build-arg ALPINE_MIRROR=https://mirrors.aliyun.com/alpine
FROM alpine:latest
ARG ALPINE_MIRROR=""
RUN if [ -n "$ALPINE_MIRROR" ]; then sed -i "s|https\?://dl-cdn.alpinelinux.org/alpine|$ALPINE_MIRROR|" /etc/apk/repositories; fi \
    && apk add --no-cache git python3 py3-pytest ripgrep
