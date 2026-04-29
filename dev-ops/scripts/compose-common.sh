#!/usr/bin/env bash

set -u

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"

COMPOSE_FILE=""
COMPOSE_CMD=()

find_compose_file() {
  if [[ -f "$SCRIPT_DIR/docker-compose.yml" ]]; then
    COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
    return 0
  fi

  if [[ -f "$SCRIPT_DIR/docker-compose.yaml" ]]; then
    COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yaml"
    return 0
  fi

  echo "未找到 docker-compose.yml 或 docker-compose.yaml，请将脚本放在 compose 文件所在目录后重试。" >&2
  exit 1
}

ensure_docker_exists() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "未检测到 docker，请先安装 Docker。" >&2
    exit 1
  fi

  if ! docker info >/dev/null 2>&1; then
    echo "检测到 docker 命令已安装，但 Docker 未启动，请先启动 Docker 后再重试。" >&2
    exit 1
  fi
}

resolve_compose_cmd() {
  ensure_docker_exists

  if docker compose version >/dev/null 2>&1; then
    COMPOSE_CMD=(docker compose)
    return 0
  fi

  if command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_CMD=(docker-compose)
    return 0
  fi

  echo "未检测到 docker compose 或 docker-compose，请先安装 Docker Compose。" >&2
  exit 1
}

list_compose_images() {
  local images_output

  if ! images_output=$("${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" config --images 2>/dev/null); then
    echo "无法解析 compose 镜像列表，请检查 $COMPOSE_FILE 是否有效。" >&2
    exit 1
  fi

  printf '%s\n' "$images_output" | while IFS= read -r image; do
    if [[ -n "$image" ]]; then
      printf '%s\n' "$image"
    fi
  done
}

collect_missing_images() {
  local image
  local missing=()

  while IFS= read -r image; do
    [[ -z "$image" ]] && continue
    if ! docker image inspect "$image" >/dev/null 2>&1; then
      missing+=("$image")
    fi
  done < <(list_compose_images)

  if [[ ${#missing[@]} -gt 0 ]]; then
    printf '%s\n' "${missing[@]}"
  fi
}

trim_trailing_cr() {
  local value="$1"
  printf '%s' "${value%$'\r'}"
}
