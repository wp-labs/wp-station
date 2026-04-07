#!/bin/bash

set -euo pipefail  # 遇到错误立即退出，并让管道失败冒泡

echo "🧪 开始运行测试覆盖率..."

run_step() {
  local message="$1"
  shift
  echo ""
  echo "$message"
  if "$@"; then
    return 0
  fi
  local code=$?
  echo "❌ ${message} 失败，退出码 ${code}"
  exit "$code"
}

# 1. 运行测试并生成 HTML 报告和文本摘要 (只运行集成测试)
run_step "📊 生成覆盖率报告..." \
  cargo llvm-cov --html --test mod -- --test-threads=1

# 3. 移动 HTML 报告到根目录
run_step "📁 整理报告文件..." bash -c 'rm -rf coverage && mv target/llvm-cov/html coverage'

# 3b. 输出覆盖率摘要
echo ""
echo "📑 生成覆盖率摘要..."
run_step "📑 导出 coverage/summary.json" cargo llvm-cov report --json --output-path coverage/summary.json
python3 - <<'PY'
import json
from pathlib import Path

root = Path(__file__).resolve().parent
summary = json.load(open("coverage/summary.json"))
files = []
total_lines = 0
total_covered = 0
for entry in summary.get("data", []):
    for file_info in entry.get("files", []):
        lines = file_info.get("summary", {}).get("lines", {})
        count = lines.get("count", 0)
        covered = lines.get("covered", 0)
        if count == 0:
            continue
        total_lines += count
        total_covered += covered
        path = Path(file_info.get("filename", ""))
        try:
            rel = path.relative_to(root)
        except ValueError:
            rel = path
        pct = covered / count * 100 if count else 0.0
        files.append((str(rel), pct, covered, count))

files.sort()
overall_pct = total_covered / total_lines * 100 if total_lines else 0.0
print(f"总行覆盖率: {overall_pct:.2f}% ({total_covered}/{total_lines})")
print("文件覆盖率概览:")
for rel, pct, covered, count in files:
    print(f"  {rel:70} {pct:6.2f}% ({covered}/{count})")
PY

# 3c. 读取 summary.json 计算覆盖率用于摘要输出
COVERAGE_PERCENT=$(python3 - <<'PY'
import json
try:
    with open("coverage/summary.json", "r") as f:
        summary = json.load(f)
    lines = summary.get("data", [{}])[0].get("totals", {}).get("lines", {})
    pct = lines.get("percent")
    if pct is None:
        raise ValueError("no percent")
    print(f"{pct:.2f}%")
except Exception:
    print("")
PY
)

# 4. 清理中间产物
echo "🧹 清理中间产物..."
cargo llvm-cov clean

echo ""
echo "✅ 完成！"
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [ -n "$COVERAGE_PERCENT" ]; then
  echo "   当前代码覆盖率: ${COVERAGE_PERCENT}"
else
  echo "   当前代码覆盖率: 未知"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
echo "📄 HTML 报告位置: ./coverage/index.html"
echo ""
echo "💡 查看报告: open coverage/index.html"
echo ""
