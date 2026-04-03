# Google Stitch MCP Integration

## Stitch la gi?

Google Stitch (stitch.withgoogle.com) la mot AI design tool cua Google Labs,
su dung Gemini models (Gemini 3 Pro / Gemini 3 Flash) de gen UI design tu text prompt.
Stitch expose mot Remote MCP server cho phep AI coding tools goi truc tiep.

## MCP Tools Reference

### Project Management

| Tool | Mo ta | Params |
|------|-------|--------|
| `create_project` | Tao project moi | `name`, `description` |
| `list_projects` | Liet ke cac project dang hoat dong | - |
| `get_project` | Lay chi tiet project | `project_id` |

### Screen Generation

| Tool | Mo ta | Params |
|------|-------|--------|
| `generate_screen_from_text` | Gen UI screen tu text prompt | `project_id`, `prompt`, `model_id` (GEMINI_3_PRO / GEMINI_3_FLASH), `device_type` (MOBILE/DESKTOP/TABLET/AGNOSTIC) |
| `list_screens` | Liet ke screens trong project | `project_id` |
| `get_screen` | Lay chi tiet screen | `project_id`, `screen_id` |

### Design Extraction

| Tool | Mo ta | Params |
|------|-------|--------|
| `extract_design_context` | Trich "Design DNA" (Tailwind palette, fonts, layout, structure) | `project_id`, `screen_id` |

> **Luu y:** Tool dung trong project la `extract_design_context`, KHONG PHAI `extract_spec`.
> Can cap nhat `STITCH_MCP_TOOL_NAME` trong `.env`.

### Virtual Tools (tu @_davideast/stitch-mcp proxy)

| Tool | Mo ta | Params |
|------|-------|--------|
| `get_screen_code` | Lay HTML/CSS cua screen | `project_id`, `screen_id` |
| `get_screen_image` | Lay screenshot base64 cua screen | `project_id`, `screen_id` |
| `build_site` | Build multi-page site tu screens | `projectId`, `routes: [{screenId, route}]` |

## Setup

### 1. Prerequisites

- Node.js 18+
- Google Cloud project co billing enabled
- `gcloud` CLI

### 2. Authentication

3 cach xac thuc:

```bash
# Cach 1: OAuth (recommended)
gcloud auth application-default login
gcloud config set project YOUR_PROJECT_ID
gcloud beta services mcp enable stitch.googleapis.com --project=YOUR_PROJECT_ID

# Cach 2: API Key (don gian nhat)
export STITCH_API_KEY="your-api-key"

# Cach 3: Dung stitch-mcp init wizard
npx @_davideast/stitch-mcp init
```

### Environment Variables

| Bien | Chuc nang |
|------|-----------|
| `STITCH_API_KEY` | API key truc tiep (bypass OAuth) |
| `STITCH_ACCESS_TOKEN` | Token da co san |
| `STITCH_USE_SYSTEM_GCLOUD` | Dung gcloud he thong thay vi bundled |
| `STITCH_PROJECT_ID` | Override project ID |
| `GOOGLE_CLOUD_PROJECT` | Project ID (alternative) |
| `STITCH_HOST` | Custom Stitch API endpoint |

### 3. MCP Proxy Setup

```bash
# Chay proxy truc tiep
npx -y @_davideast/stitch-mcp proxy

# Dang ky trong Claude Code
claude mcp add \
  -e GOOGLE_CLOUD_PROJECT=YOUR_PROJECT_ID \
  -s user stitch -- \
  npx -y @_davideast/stitch-mcp proxy
```

IDE config (VS Code, Cursor, etc):
```json
{
  "mcpServers": {
    "stitch": {
      "command": "npx",
      "args": ["@_davideast/stitch-mcp", "proxy"]
    }
  }
}
```

### 4. stitch-mcp CLI Commands

| Command | Chuc nang |
|---------|-----------|
| `init` | Setup auth, gcloud, MCP client |
| `doctor` | Kiem tra config |
| `logout` | Xoa credentials |
| `serve -p <id>` | Host screens tren Vite dev server |
| `screens -p <id>` | Browse screens trong terminal |
| `view` | Interactive browser cho projects/screens |
| `site -p <id>` | Gen Astro project tu screens |
| `snapshot` | Luu screen state ra file |
| `tool [name]` | Goi MCP tool tu CLI |
| `tool -s [name]` | Xem tool schema |
| `proxy` | Chay MCP server cho IDE |

```bash
# List all tools
npx @_davideast/stitch-mcp tool

# Xem schema cua 1 tool
npx @_davideast/stitch-mcp tool extract_design_context -s

# Goi tool truc tiep
npx @_davideast/stitch-mcp tool build_site -d '{
  "projectId": "123456",
  "routes": [
    { "screenId": "abc", "route": "/" },
    { "screenId": "def", "route": "/about" }
  ]
}'
```

## SDK (TypeScript)

### Cai dat

```bash
npm install @google/stitch-sdk
# Cho Vercel AI SDK:
npm install @google/stitch-sdk ai
```

### Core Classes

#### Stitch (Root - singleton)

```typescript
import { stitch } from "@google/stitch-sdk";
// Tu dong doc STITCH_API_KEY tu env

// Methods:
stitch.projects()                           // -> Promise<Project[]>
stitch.project(id)                          // -> Project (no API call)
stitch.listTools()                          // -> Promise<{ tools }>
stitch.callTool(name, args)                 // -> Promise<any>
```

#### Project

```typescript
const project = stitch.project("project-id");

project.generate(prompt, deviceType?)       // -> Promise<Screen>
project.screens()                           // -> Promise<Screen[]>
project.getScreen(screenId)                 // -> Promise<Screen>
```

**DeviceType:** `"MOBILE"` | `"DESKTOP"` | `"TABLET"` | `"AGNOSTIC"`

#### Screen

```typescript
const screen = await project.generate("A modern SaaS landing page", "DESKTOP");

screen.getHtml()                            // -> Promise<string> (HTML URL)
screen.getImage()                           // -> Promise<string> (screenshot URL)
screen.edit(prompt, deviceType?, modelId?)  // -> Promise<Screen>
screen.variants(prompt, options)            // -> Promise<Screen[]>
```

**ModelId:** `"GEMINI_3_PRO"` | `"GEMINI_3_FLASH"`

#### Variants

```typescript
const variants = await screen.variants("Try different styles", {
  variantCount: 3,         // 1-5
  creativeRange: "EXPLORE", // REFINE | EXPLORE | REIMAGINE
  aspects: ["COLOR_SCHEME", "LAYOUT"]
  // LAYOUT | COLOR_SCHEME | IMAGES | TEXT_FONT | TEXT_CONTENT
});

for (const v of variants) {
  const html = await v.getHtml();
  const image = await v.getImage();
}
```

#### StitchToolClient (low-level MCP)

```typescript
import { StitchToolClient } from "@google/stitch-sdk";

const client = new StitchToolClient({ apiKey: "your-key" });
const result = await client.callTool("create_project", { title: "My App" });
const tools = await client.listTools();
await client.close();
```

#### Vercel AI SDK Integration

```typescript
import { generateText, stepCountIs } from "ai";
import { google } from "@ai-sdk/google";
import { stitchTools } from "@google/stitch-sdk/ai";

const { text, steps } = await generateText({
  model: google("gemini-2.5-flash"),
  tools: stitchTools({
    include: ["create_project", "generate_screen_from_text", "get_screen"]
  }),
  prompt: "Create a project and generate a modern dashboard",
  stopWhen: stepCountIs(5),
});
```

### Code Examples

```typescript
import { stitch } from "@google/stitch-sdk";

// --- Gen screen ---
const project = stitch.project("project-id");
const screen = await project.generate("A login page with email and password");
const html = await screen.getHtml();
const imageUrl = await screen.getImage();

// --- List projects ---
const projects = await stitch.projects();
for (const p of projects) {
  const screens = await p.screens();
  console.log(p.id, screens.length, "screens");
}

// --- Edit screen ---
const edited = await screen.edit("Make the background dark, add sidebar");
const editedHtml = await edited.getHtml();

// --- MCP Proxy Server ---
import { StitchProxy } from "@google/stitch-sdk";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";

const proxy = new StitchProxy({ apiKey: "..." });
await proxy.start(new StdioServerTransport());
```

> **Caching:** `getHtml()` va `getImage()` dung cached data tu generation response.
> Neu screen load tu `screens()` hoac `getScreen()`, tu dong goi API `get_screen`.

## Pipeline Flow Hien Tai vs. Stitch Flow

### Hien tai (OpenAI + HuggingFace)

```
User prompt
  -> Design worker -> HuggingFace FLUX (gen mockup JPEG)
  -> Spec worker -> OpenAI Vision (extract spec JSON)
  -> Codegen worker -> OpenAI (gen HTML tu spec)
  -> CI -> Deploy (Vercel)
```

### Flow voi Stitch MCP

```
User prompt
  -> Design worker -> Stitch generate_screen_from_text (gen 3 variants)
     -> User chon screen
  -> Spec worker -> Stitch extract_design_context (Design DNA)
  -> Codegen worker -> Stitch get_screen_code (lay HTML that)
     HOAC -> Claude/OpenAI gen code tu Design DNA
  -> CI -> Deploy (Vercel)
```

### Loi ich cua Stitch

1. **Design chat luong cao**: Gemini 3 Pro/Flash gen UI that, khong phai random image
2. **HTML thuc te**: `get_screen_code` tra ve HTML/CSS co the deploy truc tiep
3. **Design DNA**: `extract_design_context` tra ve Tailwind palette, fonts, layout co cau truc
4. **Variants**: Gen nhieu bien the voi cac creative range khac nhau
5. **Multi-screen**: Tao nhieu screen trong 1 project, lien ket voi nhau

## Thay Doi Can Thiet De Tich Hop

### .env

```env
# Thay doi Stitch config
STITCH_API_URL=https://stitch.googleapis.com/mcp   # hoac chay qua proxy
STITCH_MCP_TOOL_NAME=extract_design_context         # doi tu extract_spec
GOOGLE_CLOUD_PROJECT=your-project-id

# Banana (design) -> co the thay bang Stitch generate_screen_from_text
# BANANA_API_URL -> khong can nua neu dung Stitch cho design
```

### mcp-gateway

1. Them Stitch MCP client (jsonrpc 2.0)
2. `handle_design_generate`: goi `generate_screen_from_text` thay vi HuggingFace
3. `handle_spec_extract`: goi `extract_design_context` thay vi OpenAI vision
4. Them endpoint moi: `/mcp/design/get-code` -> goi `get_screen_code`

### workers/design

1. Goi gateway 3 lan voi variant params thay vi seed
2. Luu `screen_id` cua moi variant vao step detail
3. User chon -> luu selected `screen_id`

### workers/spec

1. Goi `extract_design_context` voi `screen_id` da chon
2. Tra ve Design DNA (Tailwind, fonts, layout)

### workers/codegen

1. Option A: Goi `get_screen_code` lay HTML truc tiep (nhanh, chinh xac)
2. Option B: Dung Design DNA + Claude/OpenAI gen code custom (linh hoat hon)

## Pricing (Preview)

- **Flash**: 350 generations/month (free)
- **Pro**: 50 generations/month (free)
- Khong can credit card trong giai doan preview

## References

- Stitch: https://stitch.withgoogle.com
- Stitch Docs: https://stitch.withgoogle.com/docs
- MCP Setup: https://stitch.withgoogle.com/docs/mcp/setup/
- MCP Guide: https://stitch.withgoogle.com/docs/mcp/guide/
- MCP Reference: https://stitch.withgoogle.com/docs/mcp/reference/
- SDK: https://stitch.withgoogle.com/docs/sdk/
- SDK GitHub: https://github.com/google-labs-code/stitch-sdk
- MCP Proxy: https://github.com/davideast/stitch-mcp
- Blog: https://blog.google/innovation-and-ai/models-and-research/google-labs/stitch-ai-ui-design/
