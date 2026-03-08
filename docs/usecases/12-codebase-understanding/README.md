# Use Case 12: Codebase Understanding -- Real AST Analysis

### Overview

Understanding a codebase means understanding its architecture: which modules depend on which, how classes inherit, where the complexity lives, and how data flows through the system. This walkthrough parses the real `psf/requests` library (5,628 lines of Python, 18 modules) using Python's `ast` module and builds a knowledge graph in engram. No simulation -- every node and edge comes from actual source code analysis.

**What this demonstrates:**

- Real AST parsing of a production Python library
- Module, class, function, and method extraction with properties (line count, docstrings, args)
- Import dependency graph (internal + external dependencies)
- Class inheritance hierarchy (25 exception classes, adapters, auth, cookies, models)
- Inference rules: transitive dependencies and coupling detection
- Architectural exploration via graph traversal
- Text search across all code entities
- JSON-LD export of codebase knowledge

**What requires external tools:**

- Python script with `requests` library (calls the HTTP API)
- Clone of `psf/requests` from GitHub

### Prerequisites

- `engram` binary on your PATH (or `./target/release/engram`)
- Python 3.9+ with `requests` installed
- Clone of the requests repo:
  ```bash
  git clone --depth 1 https://github.com/psf/requests.git /tmp/requests-repo
  ```

### Files

```
12-codebase-understanding/
  README.md             # This file
  codebase_demo.py      # Real AST parser + engram loader
```

### Step-by-Step

#### Step 1: Clone the target repo

```bash
git clone --depth 1 https://github.com/psf/requests.git /tmp/requests-repo
```

#### Step 2: Start the engram server

```bash
engram serve codebase.brain 127.0.0.1:3030
```

#### Step 3: Run the demo

```bash
python codebase_demo.py
```

### What Happens

#### Phase 1: AST Analysis of psf/requests

Every `.py` file in `src/requests/` is parsed with Python's `ast` module. The parser extracts:

| Module | Lines | Classes | Functions | Imports |
|--------|-------|---------|-----------|---------|
| utils.py | 1,084 | 0 | 40 | 20 |
| models.py | 1,041 | 5 | 0 | 19 |
| sessions.py | 834 | 2 | 3 | 16 |
| adapters.py | 698 | 2 | 1 | 20 |
| cookies.py | 561 | 4 | 8 | 5 |
| auth.py | 314 | 4 | 1 | 11 |
| exceptions.py | 152 | 25 | 0 | 2 |
| api.py | 157 | 0 | 8 | 0 |
| ... | | | | |

**Totals:** 18 modules, 44 classes, 72 functions, 5,628 lines.

After import: **376 nodes, 514 edges** (modules, classes, methods, functions, imports, inheritance, containment).

#### Phase 2: Architectural Insights

**Class hierarchy** reveals the design:

```
sessions.Session -> SessionRedirectMixin
adapters.HTTPAdapter -> BaseAdapter
auth.HTTPBasicAuth -> AuthBase
auth.HTTPProxyAuth -> HTTPBasicAuth
auth.HTTPDigestAuth -> AuthBase
models.Request -> RequestHooksMixin
models.PreparedRequest -> RequestEncodingMixin, RequestHooksMixin
cookies.RequestsCookieJar -> cookielib.CookieJar, MutableMapping
structures.CaseInsensitiveDict -> MutableMapping
```

25 exception classes form a deep inheritance tree rooted at `RequestException -> IOError`.

**Most coupled modules** (highest import count):

```
adapters     20 imports
utils        20 imports
models       19 imports
sessions     16 imports
```

#### Phase 3: Inference -- Architectural Patterns

Two inference rules fire:

**Rule 1** (transitive dependencies): If A imports B and B imports C, derive A transitively depends on C.

**Rule 2** (coupling flag): If a module has 3+ imports, flag it as high-coupling.

Result: **63,558 rules fired, 15 modules flagged** as high-coupling (nearly all modules in a mature library import extensively).

#### Phase 4: Query the Codebase Graph

**Type queries:**
- `type:class` returns 20+ classes (adapters, auth, cookies, models, sessions, structures)
- `type:function` returns 30+ module-level functions

**Text search for "session":**
```
fn:sessions.Session.mount: conf=0.90
fn:sessions.Session.close: conf=0.90
fn:sessions.Session.__init__: conf=0.90
fn:sessions.Session.send: conf=0.90
fn:sessions.Session.post: conf=0.90
```

#### Phase 5: Architectural Exploration via Traversal

**From mod:sessions (depth=2):** 182 reachable nodes -- the heart of requests. Surfaces Session class, its methods (send, post, get, mount, close), all imported modules (adapters, auth, cookies, models, utils), and their contents.

**From class:models.Response (depth=2):** 65 reachable nodes -- the Response class with its methods (json, content, iter_content, iter_lines, apparent_encoding), linked back to the models module and its dependencies.

#### Phase 6: Explainability

```
mod:sessions (conf=0.95)
  Outgoing edges (63):
    -[imports]-> mod:os, mod:sys, mod:time, mod:collections, ...
    -[imports]-> mod:adapters, mod:auth, mod:cookies, mod:models, ...
  Incoming edges (6):
    mod:__init__ -[imports]-> mod:sessions
    class:sessions.Session -[defined_in]-> mod:sessions
    class:sessions.SessionRedirectMixin -[defined_in]-> mod:sessions
    fn:sessions.merge_setting -[defined_in]-> mod:sessions
    ...
```

#### Phase 7: Export as JSON-LD

The entire codebase graph exports as 376-node JSON-LD document, consumable by any RDF tool.

Final graph: **376 nodes, 757 edges**.

### Adapting for Other Repos

The `codebase_demo.py` script works with any Python project. Change `REPO_PATH` to point at a different source directory:

```python
# Analyze Django
REPO_PATH = "/path/to/django/django"

# Analyze Flask
REPO_PATH = "/path/to/flask/src/flask"

# Analyze your own project
REPO_PATH = "/path/to/your/project/src"
```

For non-Python codebases, replace the AST parser with:
- **Rust**: `syn` crate or `tree-sitter-rust`
- **JavaScript/TypeScript**: `tree-sitter` or `@babel/parser`
- **Go**: `go/ast` package
- **Java**: `javaparser`

### Key Takeaways

- **Real AST parsing** extracts more structure than regex or LLM analysis. Every class, function, import, and inheritance relationship is captured precisely.
- **The dependency graph reveals architecture.** `mod:sessions` is the heart of requests -- it imports 16 modules and defines the Session class that orchestrates everything.
- **Class hierarchy shows design patterns.** The auth module uses classic inheritance (AuthBase -> HTTPBasicAuth -> HTTPProxyAuth). The exception hierarchy is 25 classes deep.
- **Inference scales.** Two simple rules generated 63,558 derivations, revealing transitive dependencies across the entire import graph. This is impractical to trace manually.
- **Graph traversal answers architectural questions.** "What does Response depend on?" is a depth-2 traversal. "What imports sessions?" is an incoming-edge query. No grep needed.
- **Coupling detection is automatic.** The inference engine flagged high-coupling modules without manual thresholds or static analysis tools.
- **Code knowledge is exportable.** The JSON-LD export means codebase understanding can be shared with other tools, agents, or documentation systems.
