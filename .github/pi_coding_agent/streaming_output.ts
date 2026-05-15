/**
 * Stream Output Extension for pi coding agent
 * 
 * Source: https://github.com/yogibear54/my-own-pi-stuff/blob/main/extensions/stream-output/index.ts
 * Author: yogibear54
 * 
 * This extension adds --stream flag that outputs thinking, text, and tool content with formatting:
 * - All output goes to stderr
 * - [thinking] prefix in gray
 * - [text] prefix in cyan
 * - [tool] prefix in yellow (shows tool name, args, and results)
 * - Prefixes only appear at the start of each content block
 *
 * Usage: pi -p --stream=<value> "your prompt"
 *
 * Values (comma-separated):
 *   message   - Stream LLM response text
 *   thinking  - Stream LLM thinking text
 *   tools     - Stream tool calls and results
 *   all       - Stream everything (shorthand for message,thinking,tools)
 *
 * Examples:
 *   pi -p --stream=all "write me a poem"                         # all output to stderr
 *   pi -p --stream=message,thinking "explain recursion"          # text + thinking
 *   pi -p --stream=tools "write me a poem"                      # tool calls only
 *   pi -p --stream=all "write me a poem" 2>/dev/null             # suppress all
 *   pi -p --stream=all "write me a poem" 1>/dev/null             # text only (from stderr)
 */

import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";

// ANSI color codes
const colors = {
	gray: "\x1b[90m",
	cyan: "\x1b[36m",
	yellow: "\x1b[33m",
	red: "\x1b[31m",
	reset: "\x1b[0m",
};

// ── Helpers for parsing tool args/results ──────────────────────────────

// Safely parse args that may be a stringified JSON object or already an object
function parseArgs(args: unknown): Record<string, unknown> | null {
	if (!args) return null;
	if (typeof args === "string") {
		try {
			const parsed = JSON.parse(args);
			if (typeof parsed === "object" && parsed !== null) return parsed as Record<string, unknown>;
		} catch { /* not JSON */ }
		return null;
	}
	if (typeof args === "object") return args as Record<string, unknown>;
	return null;
}

// Extract the actual text from a content array like [{type: "text", text: "..."}]
function extractText(result: unknown): string | null {
	if (!result) return null;

	let obj: unknown = result;
	if (typeof result === "string") {
		try { obj = JSON.parse(result); } catch { return result; }
	}

	if (typeof obj === "object" && obj !== null) {
		const rec = obj as Record<string, unknown>;
		if (Array.isArray(rec.content)) {
			const texts = rec.content
				.filter((item: any) => typeof item === "object" && item.type === "text" && typeof item.text === "string")
				.map((item: any) => item.text as string);
			if (texts.length > 0) return texts.join("\n");
		}
	}
	return null;
}

// Format tool-specific concise args display
function formatToolArgs(toolName: string, args: unknown): string {
	const obj = parseArgs(args);
	if (!obj) return String(args ?? "");

	switch (toolName) {
		case "bash":
			return obj.command ? `$ ${obj.command}` : formatGeneric(obj);
		case "read": {
			let s = `${obj.path ?? "?"}`;
			if (obj.offset) s += `:${obj.offset}${obj.limit ? `-${Number(obj.offset) + Number(obj.limit)}` : "+"}`;
			return s;
		}
		case "edit": {
			const count = Array.isArray(obj.edits) ? obj.edits.length : 0;
			return `${obj.path ?? "?"} (${count} edit${count !== 1 ? "s" : ""})`;
		}
		case "write":
			return `${obj.path ?? "?"} (${formatBytes(String(obj.content ?? "").length)})`;
		default:
			return formatGeneric(obj);
	}
}

// Format tool-specific result display
function formatToolResult(toolName: string, result: unknown, maxLen = 1500): string {
	// Try extracting text from content array first
	const text = extractText(result);
	if (text !== null) {
		if (text.length === 0) return "(empty)";
		return truncateLines(text, maxLen);
	}

	// Fallback: generic formatting
	return formatGenericObj(result, maxLen);
}

// ── Formatting utilities ───────────────────────────────────────────────

function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes}B`;
	return `${(bytes / 1024).toFixed(1)}KB`;
}

function formatGeneric(obj: Record<string, unknown>): string {
	return Object.entries(obj)
		.filter(([k]) => k !== "content")
		.map(([k, v]) => {
			const s = typeof v === "string" ? v : JSON.stringify(v);
			return `${k}: ${s.length > 80 ? s.slice(0, 80) + "…" : s}`;
		})
		.join(", ");
}

function formatGenericObj(val: unknown, maxLen = 500): string {
	let s: string;
	if (typeof val === "string") {
		try {
			const parsed = JSON.parse(val);
			if (typeof parsed === "object" && parsed !== null) {
				s = JSON.stringify(parsed, null, 2);
			} else {
				s = val;
			}
		} catch { s = val; }
	} else if (typeof val === "object" && val !== null) {
		s = JSON.stringify(val, null, 2);
	} else {
		s = String(val);
	}
	if (s.length > maxLen) s = s.slice(0, maxLen) + "…";
	return s;
}

function truncateLines(text: string, maxLen: number): string {
	if (text.length <= maxLen) return text;
	const lines = text.split("\n");
	let result = "";
	let i = 0;
	for (; i < lines.length; i++) {
		const next = result + (i > 0 ? "\n" : "") + lines[i];
		if (next.length > maxLen) break;
		result = next;
	}
	if (i < lines.length) {
		const remaining = lines.length - i;
		result += `\n… (${remaining} more line${remaining !== 1 ? "s" : ""})`;
	}
	return result;
}

// Indent multiline content with hanging padding
function indentContent(text: string, indent: number): string {
	const padding = " ".repeat(indent);
	return text
		.split("\n")
		.map((line, i) => (i === 0 ? line : padding + line))
		.join("\n");
}

export default function streamOutputExtension(pi: ExtensionAPI): void {
	pi.registerFlag("stream", {
		description: "Stream output to stderr. Comma-separated values: message, thinking, tools, all",
		type: "string",
		default: "off",
	});

	type StreamOption = "message" | "thinking" | "tools";

	function getStreamOptions(): Set<StreamOption> {
		const flag = pi.getFlag("stream");
		if (!flag || flag === "off") return new Set();

		const parts = flag.split(",").map((s) => s.trim().toLowerCase());

		if (parts.includes("all")) {
			return new Set<StreamOption>(["message", "thinking", "tools"]);
		}

		const valid: StreamOption[] = ["message", "thinking", "tools"];
		return new Set<StreamOption>(parts.filter((p): p is StreamOption => valid.includes(p as StreamOption)));
	}

	let inThinkingBlock = false;
	let inTextBlock = false;
	let streamedText = "";
	let textLines = 0;

	pi.on("message_update", async (event) => {
		const opts = getStreamOptions();
		if (opts.size === 0) return;

		const assistantEvent = event.assistantMessageEvent;
		if (!assistantEvent) return;

		if (assistantEvent.type === "thinking_start") {
			if (opts.has("thinking")) {
				inThinkingBlock = true;
				process.stderr.write(`${colors.gray}[thinking] `);
			}
		} else if (assistantEvent.type === "thinking_delta") {
			if (inThinkingBlock) {
				process.stderr.write(assistantEvent.delta);
			}
		} else if (assistantEvent.type === "thinking_end") {
			if (inThinkingBlock) {
				inThinkingBlock = false;
				process.stderr.write(`${colors.reset}\n\n`);
			}
		} else if (assistantEvent.type === "text_start") {
			if (opts.has("message")) {
				inTextBlock = true;
				streamedText = "";
				textLines = 1;
				process.stderr.write(`${colors.cyan}[text] `);
			}
		} else if (assistantEvent.type === "text_delta") {
			if (inTextBlock) {
				process.stderr.write(assistantEvent.delta);
				streamedText += assistantEvent.delta;
				textLines += (assistantEvent.delta.match(/\n/g) || []).length;
			}
		} else if (assistantEvent.type === "text_end") {
			if (inTextBlock) {
				inTextBlock = false;
				process.stderr.write(`${colors.reset}\n\n`);
			}
		}
	});

	// Track running tool progress (used to overwrite previous [running] line)
	let hasRunningLine = false;

	// [calling] — concise, tool-specific args
	pi.on("tool_execution_start", async (event) => {
		const opts = getStreamOptions();
		if (!opts.has("tools")) return;
		hasRunningLine = false;
		const argsDisplay = formatToolArgs(event.toolName, event.args);
		process.stderr.write(
			`${colors.yellow}[tool] [calling] ${event.toolName}${colors.reset}\n` +
			`${colors.yellow}        ${indentContent(argsDisplay, 8)}${colors.reset}\n`,
		);
	});

	// [running] — compact progress indicator, overwrites itself to avoid spam
	pi.on("tool_execution_update", async (event) => {
		const opts = getStreamOptions();
		if (!opts.has("tools")) return;
		if (!event.partialResult) return;

		const text = extractText(event.partialResult);
		const bytes = text !== null ? text.length : JSON.stringify(event.partialResult).length;
		const sizeStr = formatBytes(bytes);

		// Overwrite previous running line if one exists
		if (hasRunningLine) {
			process.stderr.write("\x1b[1A\x1b[2K\r");
		}
		process.stderr.write(`${colors.yellow}[tool] [running] ${event.toolName} → ${sizeStr}${colors.reset}\n`);
		hasRunningLine = true;
	});

	// [result] / [error] — formatted output with real newlines
	pi.on("tool_execution_end", async (event) => {
		const opts = getStreamOptions();
		if (!opts.has("tools")) return;

		// Clear any [running] line before showing result
		if (hasRunningLine) {
			process.stderr.write("\x1b[1A\x1b[2K\r");
			hasRunningLine = false;
		}

		const resultDisplay = formatToolResult(event.toolName, event.result);
		const indented = indentContent(resultDisplay, 8);

		if (event.isError) {
			process.stderr.write(
				`${colors.red}[tool] [error] ${event.toolName}${colors.reset}\n` +
				`${colors.red}        ${indentContent(resultDisplay, 8)}${colors.reset}\n\n`,
			);
		} else {
			process.stderr.write(
				`${colors.yellow}[tool] [result] ${event.toolName}${colors.reset}\n` +
				`${colors.yellow}        ${indented}${colors.reset}\n\n`,
			);
		}
	});

	pi.on("agent_end", async () => {
		const opts = getStreamOptions();
		if (opts.size === 0) return;
		if (textLines <= 0) return;

		// In a real terminal, these ANSI codes would clear the duplicate output
		// Move cursor up N lines, then clear from cursor to end of screen
		process.stdout.write(`\x1b[${textLines}A\x1b[0J`);
	});
}