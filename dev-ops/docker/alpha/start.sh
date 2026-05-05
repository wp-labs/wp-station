#!/usr/bin/env bash

set -u

. "$(cd -- "$(dirname -- "$0")/../scripts" && pwd)/compose-common.sh"

create_env_if_missing() {
  local env_file="$SCRIPT_DIR/.env"
  local env_example_file="$SCRIPT_DIR/.env.example"
  local prompt_input="/dev/tty"
  local line=""
  local description=""
  local description_mode="interactive"
  local key=""
  local default_value=""
  local user_value=""
  local intro_shown="0"
  local generated_lines=()

  if [[ -f "$env_file" ]]; then
    echo "检测到已存在的 .env，跳过生成。"
    return 0
  fi

  if [[ ! -f "$env_example_file" ]]; then
    echo "未找到 .env.example，无法生成 .env。" >&2
    exit 1
  fi

  while IFS= read -r line || [[ -n "$line" ]]; do
    line=$(trim_trailing_cr "$line")

    if [[ -z "$line" ]]; then
      continue
    fi

    if [[ "$line" =~ ^#[[:space:]]*\$\{(.*)\}$ ]]; then
      description="${BASH_REMATCH[1]}"
      description_mode="interactive"
      continue
    fi

    if [[ "$line" =~ ^#[[:space:]]*\{(.*)\}$ ]]; then
      description="${BASH_REMATCH[1]}"
      description_mode="auto"
      continue
    fi

    if [[ "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
      key="${BASH_REMATCH[1]}"
      default_value="${BASH_REMATCH[2]}"
      user_value="$default_value"

      if [[ "$description_mode" == "interactive" ]]; then
        if [[ "$intro_shown" == "0" ]]; then
          echo "请输入以下配置，直接回车使用默认值。"
          intro_shown="1"
        fi

        if [[ ! -r "$prompt_input" ]]; then
          echo "当前终端不可交互，无法根据 .env.example 生成 .env。" >&2
          exit 1
        fi

        if [[ -n "$description" ]]; then
          read -r -p "[${description}] (默认值：${default_value}): " user_value < "$prompt_input"
        else
          read -r -p "[${key}] (默认值：${default_value}): " user_value < "$prompt_input"
        fi

        if [[ -z "$user_value" ]]; then
          user_value="$default_value"
        fi
      fi

      if [[ -n "$description" ]]; then
        generated_lines+=("# ${description}")
      fi
      generated_lines+=("${key}=${user_value}")
      description=""
      description_mode="interactive"
    fi
  done < "$env_example_file"

  : > "$env_file"
  if [[ ${#generated_lines[@]} -gt 0 ]]; then
    printf '%s\n' "${generated_lines[@]}" > "$env_file"
  fi
  echo "配置已经保存到 .env 中。"
}

start_compose() {
  echo "开始启动服务..."
  "${COMPOSE_CMD[@]}" -f "$COMPOSE_FILE" up -d
}

print_access_entries() {
  printf '\n访问入口：\n'
  printf '  - warp-station: http://localhost:8081 (宿主机端口: 8081)\n'
  printf '其他非关键服务入口：\n'
  printf '  - gitea Web: http://localhost:3000 (宿主机端口: 3000)\n'
  printf '  - gitea SSH: ssh://git@localhost:222 (宿主机端口: 222)\n'
}

main() {
  find_compose_file
  resolve_compose_cmd
  create_env_if_missing
  start_compose
  print_access_entries
}

main "$@"
