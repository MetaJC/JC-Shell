// Copyright 2024-2026 OpenJC (https://www.openjc.cc)
// 
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// 
//     http://www.apache.org/licenses/LICENSE-2.0
// 
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/**
 * Agent 模式输入分类：区分「shell 命令」与「自然语言任务」
 * 作者：JC
 *
 * 目标：用户在 Agent 输入行直接敲命令（cd / ll / git status ...）时立即交给真实 shell 执行，
 * 只有自然语言任务才发给大模型。判定分三层，从严到宽：
 * 1. 明确自然语言（含中文、含英文虚词/疑问词）→ 交给 AI；
 * 2. 明确命令（路径前缀、! 强制前缀、已知命令 + 参数特征、shell 管道/重定向）→ 直接执行；
 * 3. 其余（未知命令名，如自研 CLI / 别名）→ 'unknown'，由调用方用 is_shell_command
 *    在真实会话环境里探测（command -v / where），探到就执行，探不到才交给 AI。
 *
 * 判定倾向：宁可把一句英文白话当命令执行（shell 最多回一个 command not found），
 * 也不要把一条真命令发给大模型 —— 后者会消耗积分并多一次确认往返。
 */

/** 强制走 shell 的行首前缀（探测/启发式失手时的显式逃生口） */
export const FORCE_COMMAND_PREFIX = '!'

/** 行首即表明是命令行：相对/绝对路径、家目录、Windows 盘符、! 前缀 */
const PATH_HEAD_RE = new RegExp(
  `^(?:${FORCE_COMMAND_PREFIX}|\\.?[/\\\\]|~[/\\\\]|\\.\\\\|\\.\\./|\\.{2}[/\\\\]|[A-Za-z]:[/\\\\])`
)
/** 带扩展名的可执行脚本名（./x.sh、backup.ps1、app.exe） */
const SCRIPT_EXT_RE = /\.(?:sh|bash|zsh|ps1|psm1|bat|cmd|exe|com|py|js|ts|mjs|cjs|rb|pl|php|jar|run|bin|app|deb|rpm|msi)$/i
/** PowerShell/cmdlet 动词-名词写法（Get-ChildItem、Invoke-WebRequest、at 符号结尾的 gci 别名不含） */
const PS_CMDLET_RE = /^[A-Z][a-z]+-[A-Z][A-Za-z]+$/
/** 命令名/参数允许字符（含路径、glob、变量、引号等），用于「全行像不像命令行」判断 */
const SHELLISH_TOKEN_RE = /^["'`]?[\w@%+=:,./~\\[\]{}()*!?'"-]+["'`]?$/
/** shell 控制符（管道/串联/重定向/命令替换） */
const OPERATOR_RE = /(?:&&|\|\||[|;><`$])/

/** 命令名前缀包装器：真正的命令在它们后面 */
const WRAPPERS = new Set([
  'sudo', 'doas', 'su', 'time', 'timeout', 'nohup', 'setsid', 'nice', 'stdbuf',
  'env', 'command', 'builtin', 'xargs', 'watch', 'noglob', 'npx', 'pnpx'
])

/** 交互式/常驻前台命令（全屏程序或 REPL）：移交 shell 后需保持直通，否则 Ctrl+C 等按键会被 Agent 吞掉。
 *  只放「一定长期占屏」的程序：docker/kubectl/az 这类子命令工具跑完就退，
 *  它们的交互场景由 FOREGROUND_ARGS（exec/attach/run/-it/logs）覆盖 */
const INTERACTIVE_ALWAYS = new Set([
  'vim', 'vi', 'nvim', 'view', 'nano', 'emacs', 'pico', 'jed',
  'less', 'more', 'pg', 'man', 'info', 'top', 'htop', 'btm', 'glances', 'iotop', 'iftop', 'nethogs',
  'tmux', 'screen', 'byobu', 'ssh', 'telnet', 'ftp', 'sftp', 'mosh', 'minicom',
  'mysql', 'mariadb', 'psql', 'sqlplus', 'redis-cli', 'mongosh', 'mongo', 'sqlite3', 'clickhouse-client',
  'gdb', 'lldb', 'dlv', 'pdb', 'ipdb', 'arthas', 'jdb', 'wsl', 'ncdu', 'cmus', 'ncmpcpp',
  'vimdiff', 'nvimdiff'
])
/** 无参数时是交互式 REPL，带参数（脚本/命令）时不是 */
const INTERACTIVE_REPL = new Set([
  'python', 'python2', 'python3', 'node', 'deno', 'bun', 'irb', 'pry', 'perl', 'ruby', 'php',
  'lua', 'powershell', 'pwsh', 'cmd', 'bash', 'sh', 'zsh', 'fish', 'sqlcmd', 'iex', 'dotnet'
])
/** 长期占用前台的命令名（持续输出直到中断） */
const FOREGROUND_HEADS = new Set(['ping', 'tail', 'watch', 'journalctl', 'ngrok', 'cloudflared'])
/** 后台化标志：带这个参数说明命令会立刻返回（docker compose up -d），不该保持直通 */
const DETACH_RE = /^-{0,2}(?:d|detach|daemon)$/
/** 长期占屏的标志：跟随/交互终端/监听 */
const FG_FLAG_RE = /^-{1,2}(?:f|F|w|watch|follow|it|ti|tf|ft|interactive|service|exec|attach)$/
/** 子命令决定前后台的工具族：只有它们的子命令位才按 run/dev/exec 等判定（避开 `cp dev src` 误伤） */
const SERVICE_HEADS = new Set([
  'docker', 'podman', 'nerdctl', 'docker-compose', 'compose', 'kubectl', 'helm', 'origin',
  'npm', 'pnpm', 'yarn', 'cnpm', 'bun', 'cargo', 'mix', 'gradle', 'mvn',
  'systemctl', 'service', 'supervisorctl', 'heroku', 'az', 'aws', 'gcloud', 'fly', 'vercel',
  'python', 'python3', 'node', 'deno', 'java', 'dotnet', 'go', 'uvicorn', 'gunicorn'
])
/** 工具族里会长期前台运行的子命令 */
const FG_SUBCMD_RE = /^(?:run|dev|develop|serve|start|up|exec|attach|logs|shell|sh|bash|zsh|monitor|console|repl|debug|watch|test)$/i

/** 与自然语言高度重叠的命令词：只有带明确参数特征时才当命令执行 */
const AMBIGUOUS = new Set([
  'who', 'what', 'where', 'when', 'how', 'why', 'which', 'file', 'type', 'help', 'man',
  'find', 'list', 'show', 'get', 'set', 'check', 'run', 'do', 'make', 'test', 'touch',
  'time', 'date', 'day', 'more', 'less', 'head', 'tail', 'cut', 'sort', 'split', 'join',
  'watch', 'link', 'note', 'pass', 'group', 'user', 'dir', 'path', 'fc', 'jobs', 'wait',
  'trap', 'read', 'write', 'echo', 'printf', 'exec', 'eval', 'exit', 'logout', 'source', 'alias',
  'install', 'remove', 'delete', 'add', 'update', 'upgrade', 'status', 'log', 'logs', 'open',
  'close', 'stop', 'restart', 'clean', 'build', 'start', 'view', 'search', 'diff', 'patch', 'cp'
])

/** 英文虚词/疑问词/代词：出现在参数位说明这是一句人话而非命令行 */
const STOP_WORDS = new Set([
  'a', 'an', 'the', 'this', 'that', 'these', 'those', 'there', 'here',
  'i', 'me', 'my', 'mine', 'we', 'our', 'you', 'your', 'yours', 'he', 'she', 'it', 'its', 'they', 'them', 'their',
  'is', 'are', 'am', 'was', 'were', 'be', 'been', 'being', 'do', 'does', 'did', 'done',
  'have', 'has', 'had', 'will', 'would', 'shall', 'should', 'can', 'could', 'may', 'might', 'must',
  'to', 'of', 'for', 'and', 'or', 'nor', 'but', 'with', 'without', 'in', 'on', 'at', 'by', 'as', 'into',
  'onto', 'from', 'upon', 'about', 'above', 'below', 'under', 'over', 'between', 'against', 'during',
  'since', 'until', 'till', 'per', 'via', 'vs', 'versus', 'then', 'than', 'when', 'while', 'because',
  'if', 'else', 'so', 'such', 'that', 'some', 'any', 'many', 'much', 'few', 'all', 'both', 'each',
  'every', 'no', 'not', 'none', 'nothing', 'something', 'anything', 'everything', 'yes', 'ok',
  'please', 'pls', 'kindly', 'let', 'lets', 'give', 'tell', 'explain', 'describe',
  'what', 'how', 'why', 'who', 'whom', 'whose', 'which', 'where', 'whether', 'either', 'neither',
  'very', 'too', 'just', 'now', 'here', 'need', 'want', 'wanna', 'gonna', 'could', 'would',
  'meaning', 'means', 'difference', 'way', 'use', 'using', 'used', 'based', 'followed'
])

/** 中文自然语言标记：参数位出现未加引号的中文词即判为白话 */
const CJK_RE = /[\u3400-\u4dbf\u4e00-\u9fff\u3040-\u30ff\uac00-\ud7af]/

/**
 * 常用命令表（含 shell 内建、跨平台 CLI、PowerShell/cmd 惯用法与常见别名）。
 * 与 AMBIGUOUS 相交的词按 AMBIGUOUS 处理（需要参数特征）。
 * 表外的命令名走 is_shell_command 真实环境探测，所以此表只追求「命中即准」，不追求穷尽。
 */
const KNOWN_COMMANDS = new Set([
  // —— 目录/文件 ——
  'cd', 'chdir', 'pushd', 'popd', 'dirs', 'ls', 'dir', 'll', 'la', 'l', 'exa', 'eza', 'tree', 'pwd',
  'cat', 'tac', 'nl', 'bat', 'od', 'hexdump', 'xxd', 'strings', 'chmod', 'chown', 'chgrp', 'umask',
  'ln', 'cp', 'mv', 'rm', 'del', 'erase', 'mkdir', 'md', 'rmdir', 'rd', 'touch', 'stat', 'lsattr',
  'du', 'df', 'sync', 'rsync', 'scp', 'sftp', 'tar', 'gzip', 'gunzip', 'bzip2', 'xz', 'zstd', 'zip', 'unzip', '7z', '7za',
  'md5sum', 'sha1sum', 'sha256sum', 'cksum', 'base64', 'openssl', 'gpg', 'truncate', 'shred', 'install',
  // —— 文本处理 ——
  'grep', 'egrep', 'fgrep', 'rg', 'ag', 'ack', 'sed', 'awk', 'gawk', 'perl', 'jq', 'yq', 'tr', 'uniq',
  'wc', 'tee', 'xargs', 'paste', 'column', 'rev', 'seq', 'shuf', 'comm', 'cmp', 'diff', 'colordiff', 'patch',
  // —— 进程/系统 ——
  'ps', 'pgrep', 'pkill', 'kill', 'killall', 'jobs', 'fg', 'bg', 'disown', 'nohup', 'setsid', 'nice', 'renice',
  'top', 'htop', 'free', 'uptime', 'uname', 'hostname', 'hostnamectl', 'whoami', 'id', 'groups', 'w', 'who',
  'env', 'printenv', 'export', 'unset', 'readonly', 'history', 'alias', 'unalias', 'source', '.', 'shell',
  'systemctl', 'service', 'journalctl', 'dmesg', 'crontab', 'at', 'shutdown', 'reboot', 'halt', 'poweroff',
  'init', 'loginctl', 'timedatectl', 'localectl', 'login', 'sudo', 'doas', 'su', 'useradd', 'userdel', 'usermod',
  'groupadd', 'passwd', 'chsh', 'chfn', 'ulimit', 'sysctl', 'modprobe', 'lsmod', 'dmesg', 'lscpu', 'lsblk',
  'fdisk', 'parted', 'mkfs', 'mount', 'umount', 'df', 'blkid', 'dmsetup', 'lsof', 'fuser', 'strace', 'ltrace',
  'valgrind', 'perf', 'gdb', 'objdump', 'nm', 'readelf', 'ldd', 'ar', 'strip', 'cc', 'gcc', 'g++', 'clang',
  'clang++', 'make', 'cmake', 'ninja', 'meson', 'qmake', 'autoconf', 'automake', 'pkg-config', 'ld',
  // —— 网络 ——
  'ping', 'ping6', 'traceroute', 'tracepath', 'tracert', 'mtr', 'nslookup', 'dig', 'host',
  'ifconfig', 'ip', 'ss', 'netstat', 'route', 'arp', 'arping', 'ethtool', 'nft', 'iptables', 'ipset',
  'curl', 'wget', 'http', 'xh', 'nc', 'ncat', 'socat', 'telnet', 'ssh', 'ssh-keygen', 'ssh-copy-id', 'ssh-agent',
  'scp', 'rsync', 'nmap', 'tcpdump', 'tshark', 'minio', 'mosquitto', 'ab', 'hey',
  // —— 版本控制 / 协作 ——
  'git', 'gh', 'glab', 'svn', 'hg', 'fossil', 'repo',
  // —— 容器 / 编排 / IaC ——
  'docker', 'docker-compose', 'podman', 'buildah', 'skopeo', 'nerdctl', 'containerd', 'kubectl', 'k', 'helm',
  'kustomize', 'minikube', 'kind', 'k3s', 'terraform', 'terragrunt', 'pulumi', 'ansible', 'ansible-playbook',
  'vagrant', 'packer', 'nomad', 'consul', 'vault', 'istioctl', 'argocd', 'flux', 'rke', 'rancher',
  // —— JS/TS ——
  'npm', 'npx', 'pnpm', 'pnpx', 'yarn', 'bun', 'bunx', 'node', 'deno', 'ts-node', 'tsx', 'vue', 'vite',
  'webpack', 'rollup', 'esbuild', 'babel', 'eslint', 'prettier', 'stylelint', 'jest', 'vitest', 'mocha',
  'cypress', 'playwright', 'storybook', 'angular', 'ng', 'ionic', 'expo', 'next', 'nuxt', 'strapi', 'vercel',
  'netlify', 'wrangler', 'serverless', 'sls', 'lerna', 'nx', 'turbo', 'rush', 'yarnpkg', 'corepack',
  // —— JVM ——
  'java', 'javac', 'jar', 'jshell', 'jps', 'jstack', 'jmap', 'jcmd', 'jinfo', 'jstat', 'jhat', 'javap',
  'mvn', 'gradle', 'mvnw', 'gradlew', 'sbt', 'maven', 'kotlin', 'kotlinc', 'groovy', 'grails', 'spark-submit',
  'flink', 'hadoop', 'hive', 'presto', 'zkCli', 'kafka-topics', 'arthas-boot',
  // —— Python ——
  'python', 'python2', 'python3', 'pip', 'pip2', 'pip3', 'pipx', 'conda', 'mamba', 'poetry', 'uv', 'hatch',
  'flake8', 'pylint', 'black', 'isort', 'mypy', 'pytest', 'tox', 'jupyter', 'ipython', 'streamlit', 'django-admin',
  'flask', 'fastapi', 'gunicorn', 'uwsgi', 'celery', 'airflow', 'ruff', 'pyenv', 'virtualenv',
  // —— 其他语言 / 工具链 ——
  'go', 'gofmt', 'golangci-lint', 'cargo', 'rustc', 'rustup', 'rustfmt', 'ruby', 'gem', 'bundle', 'rake', 'rails',
  'irb', 'php', 'composer', 'laravel', 'artisan', 'dotnet', 'msbuild', 'nuget', 'csc', 'mono', 'swift', 'xcodebuild',
  'pod', 'flutter', 'dart', 'mix', 'iex', 'erl', 'elixir', 'rebar3', 'cabal', 'stack', 'ghc', 'ocaml', 'dune',
  'zig', 'nim', 'crystal', 'clojure', 'lein', 'clj', 'lua', 'luarocks', 'julia', 'matlab',
  'protoc', 'grpcurl', 'swagger-codegen', 'openapi',
  // —— 云 CLI / 数据 ——
  'aws', 'az', 'gcloud', 'gsutil', 'bq', 'doctl', 'oci', 'hcledit', 'ecs-cli', 'eb', 's3cmd', 'rclone', 'ossutil',
  'mysql', 'mysqladmin', 'mysqldump', 'mariadb', 'psql', 'pg_dump', 'pg_restore', 'vacuumdb', 'redis-cli',
  'redis-server', 'mongosh', 'mongo', 'mongodump', 'mongorestore', 'sqlite3', 'influx', 'cassandra', 'cqlsh',
  'clickhouse-client', 'clickhouse', 'elasticsearch', 'kibana', 'logstash', 'kafka', 'kafka-topics',
  'presto-cli', 'trino', 'spark-shell', 'beeline',
  // —— 包管理 / 系统运维 ——
  'apt', 'apt-get', 'apt-cache', 'dpkg', 'snap', 'yum', 'dnf', 'rpm', 'zypper', 'pacman', 'emerge', 'apk',
  'brew', 'choco', 'winget', 'scoop', 'port', 'nvm', 'sdkman', 'rbenv', 'pyenv', 'gvm',
  // —— 编辑器 / 终端 / 效率 ——
  'vim', 'vi', 'nvim', 'vimdiff', 'nano', 'emacs', 'pico', 'jed', 'ed', 'code', 'codium', 'cursor', 'idea',
  'goland', 'pycharm', 'webstorm', 'subl', 'bat', 'fzf', 'zoxide', 'ranger', 'ncdu', 'mc', 'lf', 'vifm',
  'tmux', 'screen', 'byobu', 'zsh', 'bash', 'sh', 'dash', 'fish', 'clear', 'cls', 'reset', 'exit', 'logout',
  'echo', 'printf', 'yes', 'true', 'false', 'test', '[', '[[', 'time', 'timeout', 'sleep', 'seq', 'watch',
  // —— Windows 控制台 / PowerShell ——
  'cmd', 'powershell', 'pwsh', 'conhost', 'wt', 'terminal', 'start', 'explorer', 'runas', 'icacls', 'takeown',
  'where', 'whoami', 'set', 'setx', 'getmac', 'ipconfig', 'pathping', 'path', 'assoc', 'ftype', 'date', 'time',
  'choice', 'color', 'for', 'if', 'call', 'goto', 'pause', 'prompt', 'type', 'find', 'sort', 'more',
  'tasklist', 'taskkill', 'sc', 'schtasks', 'reg', 'regedit', 'wmic', 'driverquery', 'systeminfo', 'winver',
  'dism', 'sfc', 'chkdsk', 'diskpart', 'format', 'robocopy', 'xcopy', 'msiexec', 'net', 'netsh', 'powercfg',
  'bcdedit', 'gpupdate', 'msinfo32', 'devmgmt', 'eventvwr', 'services', 'ssh-keygen',
  'Get-ChildItem', 'Get-Item', 'Get-Content', 'Get-Command', 'Get-Help', 'Get-Process', 'Get-Service',
  'Get-CimInstance', 'Get-WmiObject', 'Get-Member', 'Get-FormatData', 'Get-Location', 'Set-Location',
  'Push-Location', 'Pop-Location', 'New-Item', 'Set-Content', 'Add-Content', 'Clear-Content', 'Remove-Item',
  'Copy-Item', 'Move-Item', 'Rename-Item', 'Test-Path', 'Select-Object', 'Where-Object', 'ForEach-Object',
  'Measure-Object', 'Group-Object', 'Sort-Object', 'Compare-Object', 'Select-String', 'Invoke-Command',
  'Invoke-Expression', 'Invoke-WebRequest', 'Invoke-RestMethod', 'Start-Process', 'Stop-Process',
  'Start-Service', 'Stop-Service', 'Restart-Service', 'New-Object', 'Write-Host', 'Write-Output',
  'Set-Alias', 'Get-Alias', 'New-Alias', 'Get-History', 'Invoke-History', 'Clear-Host', 'Read-Host',
  'gci', 'ls', 'cd', 'dir', 'cat', 'type', 'echo', 'write', 'iwr', 'irm', 'sls', 'gcm', 'gps', 'ps',
  'gmo', 'saps', 'kill', 'where', 'select', 'foreach', 'format', 'ogv', 'mount', 'sc', 'set', 'get',
  // —— 别名 / 常见缩写（多来自 zsh/oh-my-zsh、PowerShell 与 git 别名，环境探测探不到，只能内置）——
  '..', '...', '....', 'md', 'rd', 'll', 'la', 'lll', 'gst', 'gco', 'gb', 'gd', 'gl', 'gc', 'gaa', 'gcs', 'gs',
  'gdh', 'gt', 'glog', 'kg', 'kd', 'kf', 'serve', 'http-server', 'live-server'
])

/** 已知命令 + 别名 + 内建（探测兜底用） */
const KNOWN = new Set<string>([...KNOWN_COMMANDS])

/** 单个 token 是否为「参数特征」（选项 / 路径 / 变量 / glob / 数字 / 重定向） */
function isArgEvidence(raw: string): boolean {
  const t = raw.replace(/^["'`]|["'`]$/g, '')
  if (!t) return false
  if (/^-/.test(t)) return true                       // 选项：-l / --force / -it
  if (/[\\/]/.test(t)) return true                    // 路径分隔符
  if (/^[~$@%`]/.test(t)) return true                 // 变量 / 家目录 / 命令替换
  if (/[*?[\]{}]/.test(t)) return true                // glob
  if (/[<>&|]/.test(t)) return true                   // 重定向 / 串联
  if (/\.(?:sh|py|js|ts|ps1|bat|cmd|exe|jar|log|txt|json|ya?ml|toml|xml|csv|md|conf|cfg|ini|env|sock|pid)$/i.test(t)) return true
  if (/^\d+(\.\d+)*[a-z%]{0,3}$/i.test(t)) return true // 数字（可带单位）
  if (/^[A-Za-z_][A-Za-z0-9_]*=/.test(t)) return true  // KEY=VAL 传参
  return false
}

/** 该行是否明确是「一句人话」：行首/参数位出现中文，或参数位出现英文虚词 */
function looksLikeProse(tokens: LineToken[]): boolean {
  for (let i = 0; i < tokens.length; i++) {
    const tk = tokens[i]
    // 引号内是参数值；选项/重定向/glob/路径（含中文目录名）都是合法命令行成分，不参与白话判定
    if (tk.quoted || /^-/.test(tk.text) || /[<>|;&]/.test(tk.text) || isArgEvidence(tk.text)) continue
    // 中日韩文字无论出现在行首还是参数位都说明是人话（命令名不会用中文）
    if (CJK_RE.test(tk.text)) return true
    // 英文虚词只查参数位：行首虚词可能是命令名（do/type/echo 等）
    if (i === 0) continue
    const lower = tk.text.toLowerCase()
    if (/^[a-z]+$/.test(lower) && STOP_WORDS.has(lower)) return true
  }
  return false
}

/** 拆 token（保留引号信息，引号内不参与判定） */
export interface LineToken {
  text: string
  quoted: boolean
}

export function tokenizeLine(line: string): LineToken[] {
  const out: LineToken[] = []
  let cur = ''
  let quote = ''
  let quoted = false
  for (const ch of line) {
    if (quote) {
      cur += ch
      if (ch === quote) quote = ''
      continue
    }
    if (ch === '"' || ch === "'" || ch === '`') {
      quote = ch
      quoted = true
      cur += ch
      continue
    }
    if (/\s/.test(ch)) {
      if (cur) { out.push({ text: cur, quoted }); cur = ''; quoted = false }
      continue
    }
    cur += ch
  }
  if (cur) out.push({ text: cur, quoted })
  return out
}

/** 剥掉 sudo / time / env 等包装器，露出真正的命令词（顺带跳过包装器自身的选项与时长参数） */
function unwrapHead(tokens: LineToken[]): { head: string; rest: LineToken[] } {
  let i = 0
  let wrapped = false
  while (i < tokens.length) {
    const t = tokens[i].text.replace(/^["'`]|["'`]$/g, '')
    if (WRAPPERS.has(t.toLowerCase())) { wrapped = true; i++; continue }
    // 包装器后面的选项（-u）/时长（10s）/数字参数都不可能是命令名
    if (wrapped && (/^-/.test(t) || /^[\d.]+$/.test(t))) { i++; continue }
    break
  }
  return { head: tokens[i]?.text ?? '', rest: tokens.slice(i + 1) }
}

/** 命令行外观（全 ASCII、无空格词、字符集受限）——用于「未知命令名」是否值得探测 */
function looksShellish(tokens: LineToken[]): boolean {
  return tokens.every(t => !t.quoted ? SHELLISH_TOKEN_RE.test(t.text) : t.text.length > 1)
}

/** 是否是需要保持 Shell 直通的交互式/常驻前台命令（否则回车后就交还 Agent） */
export function isForegroundCommand(line: string): boolean {
  const tokens = tokenizeLine(line.trim())
  if (!tokens.length) return false
  const { head, rest } = unwrapHead(tokens)
  const h = head.toLowerCase().replace(/^["'`]|["'`]$/g, '')
  if (INTERACTIVE_ALWAYS.has(h) || FOREGROUND_HEADS.has(h)) return true
  if (INTERACTIVE_REPL.has(h) && rest.length === 0) return true
  const args = rest.filter(t => !t.quoted).map(t => t.text.toLowerCase())
  // -d / --detach：后台跑，立刻返回提示符，不进 sticky
  if (args.some(a => DETACH_RE.test(a))) return false
  if (args.some(a => FG_FLAG_RE.test(a))) return true
  if (SERVICE_HEADS.has(h) && args.some(a => FG_SUBCMD_RE.test(a))) return true
  return args.some(a => /(?:server|daemon|gateway|watcher)$/.test(a))
}

/** 输入行首是否已是「无可辩驳的命令行」——打字过程中即可移交 shell（获得原生补全/历史） */
export function isConfidentCommandLine(line: string): boolean {
  if (PATH_HEAD_RE.test(line)) return true
  const m = /^(\S+)\s+\S/.exec(line)
  if (!m) return false // 首个词未打完 / 参数位还没内容：继续本地缓冲，避开「ls 是什么意思」这类误判
  const head = m[1].toLowerCase()
  if (WRAPPERS.has(head)) return true // sudo / time ... 后面必是命令
  if (SCRIPT_EXT_RE.test(m[1]) || PS_CMDLET_RE.test(m[1])) return true
  const tokens = tokenizeLine(line.trim())
  if (looksLikeProse(tokens)) return false // 参数位出现白话（中文/虚词）：不抢这条线
  if (AMBIGUOUS.has(head)) return false // 有歧义的词留给回车判定
  return KNOWN.has(head) || OPERATOR_RE.test(line)
}

export type LineKind = 'command' | 'natural' | 'unknown'

/**
 * 分类一行 Agent 输入。
 * @returns kind：command = 直接执行；natural = 交给大模型；unknown = 需真实环境探测后决定
 */
export function classifyLine(line: string): { kind: LineKind; head: string } {
  const text = line.trim()
  if (!text) return { kind: 'natural', head: '' }
  if (text.startsWith(FORCE_COMMAND_PREFIX)) return { kind: 'command', head: text.slice(1) }

  const tokens = tokenizeLine(text)
  const { head, rest } = unwrapHead(tokens)
  const h = head.toLowerCase()
  if (!h) return { kind: 'natural', head: '' }

  // 结构上必然是命令行：相对/绝对路径、PowerShell 动词-名词、带扩展名的脚本
  if (PATH_HEAD_RE.test(head) || PS_CMDLET_RE.test(head) || SCRIPT_EXT_RE.test(head)) return { kind: 'command', head }
  // 行首就是中日韩文字（如「帮我安装 java 17」）：这是自然语言任务，不是命令名
  if (CJK_RE.test(head)) return { kind: 'natural', head }
  const evidence = rest.some(t => isArgEvidence(t.text)) || OPERATOR_RE.test(text)
  const prose = looksLikeProse(tokens)
  // 无歧义命令词（cd/ls/git/docker... 几乎不会是英文白话开头）
  const sure = KNOWN.has(h) && !AMBIGUOUS.has(h)

  if (sure && evidence) return { kind: 'command', head }   // git log --oneline -5
  if (prose) return { kind: 'natural', head }              // who are you / 看看这个报错
  if (evidence) return { kind: 'command', head }           // mycli --config x.yml
  if (sure) return { kind: 'command', head }               // 裸命令：ll / clear / history
  // 歧义词（find/man/who...）与未知命令名：看真实环境里认不认得（探测失败则交给大模型）
  return looksShellish(tokens) ? { kind: 'unknown', head } : { kind: 'natural', head }
}

/** 去掉强制前缀，得到实际要交给 shell 的文本（只删行首 ! ，保留行尾空格供继续输入） */
export function stripForcePrefix(line: string): string {
  return line.replace(/^\s*!/, '')
}
