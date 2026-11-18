# mem

AI 에이전트들이 세션을 넘어 공유하는 **프로젝트 메모리**. 결정(decision), 규칙(convention), 함정(gotcha),
사실(fact), 인수인계(handoff)를 로컬 SQLite 파일 하나에 저장하고, 가벼운 CLI로 검색합니다.

- 서버·네트워크·외부 API 없음. 명령 하나가 프로세스 하나 (검색 약 10 ms, 5천 개 메모 기준).
- 키워드 검색(SQLite FTS5) + 한국어 조사 처리 + 동의어. 임베딩(EmbeddingGemma)은 선택 빌드.
- 같은 주제의 메모가 둘 이상 "현재"로 남지 않게 저장 단계에서 막고(supersede / keep-both), 비슷한 메모는 검색 결과에서 한 묶음으로 접습니다.
- 에이전트용 사용법은 [SKILL.md](SKILL.md), 설계와 결정 기록은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/DECISIONS.md](docs/DECISIONS.md).

버전 0.0.1. Python 으로 만든 `mem` v5.3 을 Rust 로 옮긴 것으로, 키워드 검색 경로의 출력이 v5.3 과 바이트 단위로 같음을 확인한 뒤 Python 판은 정리했습니다(D-22).

## 설치

### winget (Windows, 배포 후)

```powershell
winget install <Publisher>.Mem      # 식별자는 첫 배포 때 정함 (RELEASING.md)
mem where
```

### 릴리스 압축 파일

GitHub 릴리스의 `mem-<버전>-<target>.zip|tar.gz` 를 풀어 `mem`(`mem.exe`)을 PATH 에 두거나, 프로젝트마다
`<프로젝트>/.bin/mem` 으로 복사합니다. 임베딩 빌드는 `mem-embed-*` 입니다.

### 소스에서 빌드

```sh
cargo build --release                      # 키워드 검색 빌드, target/release/mem (약 5 MB)
cargo build --release --features embed     # 임베딩 빌드 (llama.cpp 를 함께 컴파일: CMake, C++ 컴파일러 필요)
```

임베딩 빌드는 CPU 최적화를 꼭 켜세요. 안 켜면 llama.cpp 가 SSE2 기준으로 빌드되어 몇 배 느립니다.

```sh
RUSTFLAGS="-C target-cpu=native" cargo build --release --features embed       # 이 PC 전용
RUSTFLAGS="-C target-cpu=x86-64-v3" cargo build --release --features embed    # AVX2 PC 공용
```

Windows 에서 임베딩 빌드: `LLAMA_STATIC_CRT=1` 도 설정하세요(`.cargo/config.toml` 의 정적 CRT 와 맞춤).

## 저장소 위치

호출마다 아래 순서로 저장소(SQLite 파일)를 정합니다. `mem where` 로 확인할 수 있고, `where` 는 아무것도 만들지 않습니다.

1. `MEM_DB` 환경 변수
2. 실행 파일이 `<프로젝트>/.bin/` 안에 있으면(또는 실행 파일 폴더의 상위에 `.mem/` 이 있으면) → `<프로젝트>/.mem/mem.db`
3. PATH 에 설치된 경우(winget 등) → 현재 폴더에서 위로 올라가며 처음 만나는 `.mem/mem.db`
4. 아직 없으면 → `.git` 이 있는 가장 가까운 상위 폴더(없으면 현재 폴더)에 `.mem/mem.db` 를 새로 만듦

같은 폴더에 `calls.log`(호출 기록, JSON 한 줄씩)와 임베딩 모델 `model.gguf`(선택)를 둡니다.
한 저장소에 여러 프로젝트를 `--project` 로 나눠 넣을 수 있습니다.

## 메모 모양

| 필드 | 뜻 |
|---|---|
| id | `<project 앞 2글자>-<kind 앞 4글자>-<번호>`, 예: `sh-deci-003` |
| kind | `decision`, `convention`, `gotcha`, `fact`, `handoff`, `topic` (자유 문자열이지만 이 여섯을 씀) |
| title | 주제(값이 아니라 대상). 예: `Wishlist alert channel` |
| summary | 한 줄 요약(값). 예: `#shop-alerts 로 보낸다` |
| body | 자세한 내용(선택) |
| tags | 쉼표 구분. 모듈 이름을 넣으면 모듈 카드에 쓰임 |
| status | `current` / `superseded`(새 메모로 대체됨) / `retired`(틀림·중복으로 숨김) |
| source | `user`(사용자가 말함) / `agent`(에이전트가 스스로 정함) |

`topic` 은 프로젝트 전체 규칙을 주제별로 한 메모에 모은 것입니다(제목 `<주제> rules`, 본문은 `- 규칙 (since 날짜)` 한 줄씩).

## 명령어

모든 명령은 결과를 표준 출력에, 오류를 `mem: error: ...` 한 줄로 표준 오류에 씁니다. 종료 코드는 성공 0,
실행 오류 1, 인자 오류 2. 옵션 이름은 앞부분만 써도 됩니다(`--proj`). `-` 로 시작하는 값(`"- 항목"`)도 받습니다.

### brief — 세션 시작 요약

```sh
mem brief --project shop
```

현재 메모 수(종류별), 주제 메모(topic) 목록, 최근 인수인계 2개, 최근 바뀐 메모 8개.

```
# shop: 2 current notes (decision 1, convention 1)
Before deciding: mem search --project shop --q ... --kw ... · rules overview: mem list --project shop --kind topic,convention,decision,gotcha
## Recently changed
- sh-deci-003 · decision · Wishlist alert channel
- sh-conv-002 · convention · Cart error code prefix
```

### search — 현재 메모 검색

```sh
mem search --project shop --q "찜 알림은 어디로 보내?" --kw "wishlist,alert,channel,알림"
```

| 옵션 | 설명 |
|---|---|
| `--q` (필수) | 사용자의 요청이나 질문을 그대로 |
| `--kw` | 추가 키워드, 쉼표 구분: 핵심어, 동의어, 영어·한국어 표기 둘 다 |
| `--project` | 이 프로젝트만 (없으면 전체) |
| `--kind` | 이 종류만, 쉼표 구분 |
| `--limit` | 결과 묶음 수 (기본 5, `MEM_LIMIT`) |

출력: 순위별 메모(제목, 요약; 주제 메모는 본문까지). 같은 패턴의 메모(모듈마다 하나씩 있는 규칙 등)는 첫 메모
아래에 제목만 접혀 나옵니다(최대 `MEM_SIMSHOW` 개). 질의가 관련 주제 메모를 건드리면 맨 앞에 붙고, 질의에 모듈 이름이 있으면 그 모듈의
다른 현재 메모가 한 줄씩 나옵니다. 같은 주제의 현재 메모가 둘이면 `⚠ … SAME SUBJECT`, 내용이 겹치는 현재 메모끼리는
`! overlaps with …` 줄이 붙습니다. `get` 으로 전체를 보세요.

```
[1] sh-deci-003 · decision · 2026-11-18
    Wishlist alert channel
    ↳ 이제 #shop-alerts 로 보낸다
```

검색되는 단어: 2글자 이상, 조사(은/는/이/가/을/를/의/에/에서/로/으로/와/과/도/만 …)는 떼고, 흔한 불용어는 뺍니다.
에이전트가 넣은 동의어(`mem syn show`)로 넓힙니다. **알려진 한계**: 1글자 단어(찜, 팀, C)와 조사처럼 끝나는 2글자 단어 일부(평가,
회의)는 지금 검색어에서 빠집니다. `--kw` 에 다른 표기(찜→wishlist)를 함께 주세요. 개선안은 docs/DECISIONS.md D-14.

### list — 개요용 목록

```sh
mem list --project shop --kind topic,convention,decision,gotcha
mem list --project shop --status superseded --full
```

| 옵션 | 설명 |
|---|---|
| `--project` | 이 프로젝트만 |
| `--kind` | 쉼표 구분 |
| `--status` | `current`(기본) / `superseded` / `retired` / `all` |
| `--tag` | 이 태그가 붙은 메모 |
| `--source` | `user` / `agent` |
| `--full` | 요약도 출력 |
| `--limit` | 최대 줄 수 (기본 200) |

같은 패턴의 메모는 한 줄로 접힙니다(`+ 16 same pattern, e.g. …`). 주제 검색에는 `search` 를 쓰세요.

### get — 메모 전체 보기

```sh
mem get sh-deci-003 sh-conv-002
```

```
sh-deci-003 · decision · 2026-11-18 · shop · current · source user
Wishlist alert channel
이제 #shop-alerts 로 보낸다

tags: wishlist,alert
```

첫 줄의 상태 뒤에 대체된 메모는 `superseded → <새 id>`, 숨긴 메모는 `retired (<이유>)` 가 붙습니다.

### add — 메모 저장

```sh
mem add --project shop --kind decision --title "Wishlist alert channel" \
        --summary "찜 알림 장애는 #wishlist-alerts 로" --tags "wishlist,alert"
```

| 옵션 | 설명 |
|---|---|
| `--project`, `--kind`, `--title`, `--summary` (필수) | |
| `--body` | 자세한 내용 |
| `--tags` | 쉼표 구분 |
| `--supersedes <id>[,<id>…]` | 이 메모가 대체하는 옛 메모 (옛 메모는 `superseded`) |
| `--source user\|agent` | 기본 `user`. 에이전트가 스스로 정한 것은 `agent` |
| `--force` | 중복처럼 보여도 저장 |
| `--keep-both` | 같은 주제의 현재 메모가 있어도 둘 다 유효 (추가된 사실일 때) |

저장 전에 검사합니다.

- **중복**: 제목이 같거나 내용이 거의 같으면 `NOT SAVED: looks like a duplicate of <id>` → `mem edit`, `--supersedes`, `--force` 중 선택.
- **같은 주제**(decision/convention/gotcha/fact): 같은 대상의 현재 메모가 있으면 `NOT SAVED: …` 와 함께 후보를 보여줍니다.
  제목에서 값을 뺀 단어가 같으면 같은 주제이고, 동의어로 넣은 말은 같은 단어로 봅니다(Wishlist alert channel = 위시리스트 알림 채널). 단어가 하나 더 붙은 제목은 다른 주제로 봅니다.
  바뀐 것(이제, ~만, 대신, 더 이상, 옮겼어 / now, only, instead)이면 `--supersedes <id>`, 더해진 것(~도, 에 더해 / also)이면 `--keep-both`.
- 저장되면 `saved <id>`, 대체했으면 `saved <id> (superseded <id>)`, 비슷한 메모가 있으면 아래에 몇 줄 보여줍니다.

### add-batch — 여러 메모 한 번에

```sh
mem add-batch --check --project shop notes.jsonl    # 미리 보기: 저장 안 함
mem add-batch --project shop notes.jsonl            # 저장
cat notes.jsonl | mem add-batch --project shop -    # 표준 입력
```

파일은 한 줄에 JSON 객체 하나:

```json
{"kind": "fact", "title": "Cart worker count", "summary": "2", "tags": "cart"}
{"kind": "decision", "title": "Wishlist alert channel", "summary": "#shop-alerts", "supersedes": "sh-deci-001"}
{"kind": "fact", "title": "Cart retry policy", "summary": "3회", "keep_both": true, "source": "agent"}
```

필드: `kind`, `title`, `summary` (필수), `body`, `tags`, `supersedes`(문자열 "a,b" 또는 목록 ["a","b"]),
`source`, `keep_both`, `force`. `--check` 는 줄마다 `new` / `DUPLICATE of …`, `! SAME SUBJECT …`(supersedes 나
keep_both 를 정해야 함), `~ same topic` / `~ similar` 를 보여줍니다. 잘못된 줄은 `NOT SAVED: bad line (…)` 로 표시하고 다음 줄을 계속합니다.

```
1: new · Cart worker count
     ~ same topic: sh-conv-002 · Cart error code prefix
2: DUPLICATE of sh-conv-002 · Cart error code prefix
     ! SAME SUBJECT, decide supersedes or keep_both: sh-conv-002 · Cart error code prefix — 장바구니 에러 코드는 CART- 로 시작
```

### edit — 작은 수정

```sh
mem edit sh-conv-002 --summary "장바구니 에러 코드는 CART- 로 시작 (예: CART-404)"
mem edit sh-conv-002 --append "- 2026-11 부터 결제 쪽도 같은 규칙"
```

옵션: `--title`, `--summary`, `--body`, `--tags`, `--kind`, `--append`(본문 끝에 한 줄 추가). 결정이 바뀐 경우는
edit 대신 `add --supersedes` 로 새 메모를 만드세요(이력이 남음).

### retire — 틀린·중복 메모 숨기기

```sh
mem retire sh-fact-010 sh-fact-011 --reason "duplicate of sh-fact-009"
```

메모는 지우지 않고 `retired` 로 남깁니다(`list --status retired`, `get` 으로 볼 수 있음).

### syn — 검색 동의어

```sh
mem syn show                                # 단어마다 동의어 한 줄
mem syn add wishlist 위시리스트 찜목록        # 서로 동의어로 등록 (소문자로 저장)
```

새 저장소에는 동의어가 하나도 없습니다. 프로젝트마다 쓰는 말이 다르므로 코드에 넣지 않고, 에이전트가 SKILL.md 지시에 따라
프로젝트에서 쓰는 다른 표기(영어·한국어, 약어, 모듈 별명)를 그때그때 넣습니다(D-20). 동의어는 저장소 전체(모든 프로젝트)에 적용됩니다.
동의어는 검색 확장과 저장 시 같은 주제 검사 양쪽에 쓰입니다(D-21). 1글자 단어는 검색어에서 빠지므로(D-14) `syn add` 에는 2글자 이상 단어를 쓰세요.

### import — 저장소 옮기기·합치기

```sh
mem import notes.jsonl
```

한 줄에 JSON 객체 하나, 모든 열을 그대로(id 유지): `id`, `project`, `kind`, `date`, `title`, `summary` (필수),
`body`, `tags`, `status`(기본 current), `superseded_by`, `source`(기본 user). 한 트랜잭션이라 중간에 실패하면 아무것도 들어가지 않습니다.

### index — 임베딩 만들기 (임베딩 빌드만)

```sh
mem index      # 임베딩이 없는 메모를 모두 임베딩. 출력: indexed <개수>
```

모델(`model.gguf`)이 있으면 `add` 는 저장할 때 임베딩도 함께 넣습니다. `index` 는 처음 켤 때나 키워드 빌드로 저장한 메모를 채울 때 씁니다.

### where — 어떤 저장소를 쓰는지

```sh
mem where
```

```
db: /home/me/repo/.mem/mem.db (exists)
  chosen by: nearest .mem/ from the current folder
model: /home/me/repo/.mem/model.gguf (not found)
embeddings: off (no model file)
version: 0.0.1 (keyword-only build)
```

## 임베딩 (선택)

`--features embed` 빌드에 EmbeddingGemma 300M Q8_0 GGUF(약 330 MB, 별도 다운로드)를 `.mem/model.gguf` 로 두거나
`MEM_MODEL` 로 지정하면 켜집니다. 검색은 키워드 결과와 의미 검색 결과를 번갈아 섞습니다(interleave). 모델을 읽지
못하면 그 호출은 경고 한 줄과 함께 키워드 검색으로 돌아갑니다. 비용: 모델 로드 약 0.3 초, 검색 약 0.4 초, 메모리 약 400 MB.
500~수천 개 메모에서는 키워드 검색만으로도 정확도가 충분해서 기본은 꺼져 있습니다(D-06).

## 환경 변수

| 변수 | 기본 | 설명 |
|---|---|---|
| `MEM_DB` | (위 규칙) | 저장소 파일 경로 |
| `MEM_MODEL` | `<저장소 폴더>/model.gguf` | 임베딩 모델 |
| `MEM_NO_EMBED` | | 값이 있으면 임베딩 끔 |
| `MEM_MODE` | `interleave` | 임베딩 결합 방식: `interleave`, `fusion`, `fusion+fts2` (실험용) |
| `MEM_LIMIT` | 5 | search 결과 묶음 수 |
| `MEM_NO_SYN` | | 값이 있으면 동의어 확장 끔 |
| `MEM_NO_GROUP` | | 값이 있으면 같은 패턴 묶기 끔 |
| `MEM_CARD` | 6 | 모듈 카드 줄 수 |
| `MEM_TOPIC_SHOW` | 1500 | 검색에 붙는 주제 메모 본문 최대 글자 수 |
| `MEM_SIMSHOW` | 8 | search 결과 아래에 접어 보여주는 같은 패턴 메모 수 |
| `MEM_TIMING` | | 값이 있으면 임베딩 시간 출력 (표준 오류) |

## 개발

```sh
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release                       # 단위 + CLI 통합 + golden(출력 스냅샷 145건)
MEM_BLESS=1 cargo test --release --test golden   # 출력을 의도적으로 바꾼 뒤 스냅샷 갱신 (diff 확인 후 커밋)
MEM_TEST_MODEL=/path/embeddinggemma-300M-Q8_0.gguf cargo test --release --features embed
```

에이전트가 이 코드를 고칠 때는 [AGENTS.md](AGENTS.md) 부터 읽으세요. 배포는 [RELEASING.md](RELEASING.md).

## 라이선스

아직 정하지 않았습니다(첫 공개 배포 전에 정해야 함, RELEASING.md).
