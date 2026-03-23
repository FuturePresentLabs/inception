#!/usr/bin/env node
/**
 * Inception channel for Claude Code - distributed agent session management.
 *
 * Self-contained MCP server with session orchestration, status tracking, and
 * rich metrics. State lives in ~/.claude/channels/inception/.env
 */
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema, } from "@modelcontextprotocol/sdk/types.js";
import http from "http";
import { readFileSync, writeFileSync, mkdirSync, chmodSync } from "fs";
import { homedir } from "os";
import { join } from "path";
import WebSocket from "ws";
const STATE_DIR = join(homedir(), ".claude", "channels", "inception");
const ENV_FILE = join(STATE_DIR, ".env");
// Load ~/.claude/channels/inception/.env into process.env. Real env wins.
// Plugin-spawned servers don't get an env block — this is where config lives.
try {
    // Token is a credential — lock to owner. No-op on Windows (would need ACLs).
    chmodSync(ENV_FILE, 0o600);
    for (const line of readFileSync(ENV_FILE, "utf8").split("\n")) {
        const m = line.match(/^(\w+)=(.*)$/);
        if (m && process.env[m[1]] === undefined)
            process.env[m[1]] = m[2];
    }
}
catch { }
// Configuration from environment (or defaults)
let REGISTRY_URL = process.env.INCEPTION_REGISTRY_URL || "http://localhost:8080";
let TOKEN = process.env.INCEPTION_TOKEN || "";
const HOOK_PORT = parseInt(process.env.INCEPTION_HOOK_PORT || "18081");
// State
let currentSessionId = null;
let wsConnection = null;
let messageQueue = [];
let isWsConnected = false;
let sessionMetrics = {
    totalToolsUsed: 0,
    totalFilesEdited: 0,
    totalCommandsRun: 0,
    sessionStartTime: new Date(),
    lastActivityTime: new Date(),
    activities: [],
};
// Tool definitions
const ATTACH_TOOL = {
    name: "inception_attach",
    description: "Attach to a remote Claude Code session. Spawns a new session if no ID provided.",
    inputSchema: {
        type: "object",
        properties: {
            session_id: {
                type: "string",
                description: "Optional session ID to attach to. If omitted, spawns a new session.",
            },
        },
    },
};
const DETACH_TOOL = {
    name: "inception_detach",
    description: "Detach from remote session and return to local",
    inputSchema: {
        type: "object",
        properties: {
            close: {
                type: "boolean",
                description: "If true, terminate the remote session",
                default: false,
            },
        },
    },
};
const STATUS_TOOL = {
    name: "inception_status",
    description: "Show current attachment status and available sessions",
    inputSchema: {
        type: "object",
    },
};
const CONFIGURE_TOOL = {
    name: "inception_configure",
    description: "Configure the Inception registry endpoint (URL, port, DNS)",
    inputSchema: {
        type: "object",
        properties: {
            registry_url: {
                type: "string",
                description: "Full registry URL (e.g., https://inception.example.com:8080)",
            },
            token: {
                type: "string",
                description: "Optional auth token for the registry",
            },
        },
    },
};
const REPLY_TOOL = {
    name: "reply",
    description: "Send a reply message back to the Inception registry",
    inputSchema: {
        type: "object",
        properties: {
            content: {
                type: "string",
                description: "Message content to send",
            },
            reply_to: {
                type: "string",
                description: "ID of the message being replied to (from inbound meta)",
            },
        },
        required: ["content"],
    },
};
const VERDICT_TOOL = {
    name: "verdict",
    description: "Approve or deny a pending permission request (for remote tool approval)",
    inputSchema: {
        type: "object",
        properties: {
            request_id: {
                type: "string",
                description: "The 5-letter permission request ID (e.g., 'abcde')",
            },
            decision: {
                type: "string",
                description: "'allow' to approve, 'deny' to reject",
                enum: ["allow", "deny"],
            },
        },
        required: ["request_id", "decision"],
    },
};
const METRICS_TOOL = {
    name: "inception_metrics",
    description: "Get detailed session metrics and activity history",
    inputSchema: {
        type: "object",
        properties: {
            format: {
                type: "string",
                description: "Output format: summary, detailed, or json",
                enum: ["summary", "detailed", "json"],
                default: "summary",
            },
        },
    },
};
const UPDATE_STATUS_TOOL = {
    name: "inception_update_status",
    description: "Update session status, agent state, and progress",
    inputSchema: {
        type: "object",
        properties: {
            status: {
                type: "string",
                description: "Session status: spawning, idle, busy, disconnected, terminated",
            },
            agent_state: {
                type: "string",
                description: "Agent state: idle, thinking, executing, waiting_for_user, error",
            },
            progress: {
                type: "number",
                description: "Progress percentage (0.0 to 1.0)",
            },
            current_task: {
                type: "string",
                description: "Description of current task",
            },
        },
    },
};
// Create server with channel capabilities
const server = new Server({
    name: "inception",
    version: "0.1.0",
});
// List available tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
        tools: [ATTACH_TOOL, DETACH_TOOL, STATUS_TOOL, CONFIGURE_TOOL, UPDATE_STATUS_TOOL, METRICS_TOOL, REPLY_TOOL, VERDICT_TOOL],
    };
});
// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    switch (name) {
        case "inception_attach":
            return handleAttach(args);
        case "inception_detach":
            return handleDetach(args);
        case "inception_status":
            return handleStatus();
        case "inception_configure":
            return handleConfigure(args);
        case "inception_update_status":
            return handleUpdateStatus(args);
        case "inception_metrics":
            return handleMetrics(args);
        case "reply":
            return handleReply(args);
        case "verdict":
            return handleVerdict(args);
        default:
            throw new Error(`Unknown tool: ${name}`);
    }
});
// Permission relay: Handle permission requests from Claude Code
// This is called when Claude wants to use a tool that requires approval
// Store pending permission requests
const pendingPermissions = new Map();
// Function to receive verdicts from registry
async function handlePermissionVerdict(requestId, allowed) {
    const pending = pendingPermissions.get(requestId);
    if (!pending) {
        console.error(`No pending permission request for ID: ${requestId}`);
        return;
    }
    // Send verdict back to Claude Code
    try {
        await server.notification({
            method: "notifications/claude/channel/permission",
            params: {
                request_id: requestId,
                behavior: allowed ? "allow" : "deny",
            },
        });
        console.error(`Permission ${requestId}: ${allowed ? "allowed" : "denied"}`);
        pending.resolve(allowed);
    }
    catch (error) {
        console.error("Failed to send permission verdict:", error);
    }
}
async function handleAttach(args) {
    try {
        let sessionId = args.session_id;
        if (!sessionId) {
            // Spawn new session
            const response = await fetch(`${REGISTRY_URL}/v1/sessions`, {
                method: "POST",
                headers: {
                    "Content-Type": "application/json",
                    "Authorization": `Bearer ${TOKEN}`,
                },
                body: JSON.stringify({
                    agent_type: "claude_code",
                    capabilities: ["rust", "python", "typescript"],
                }),
            });
            if (!response.ok) {
                throw new Error(`Failed to spawn session: ${response.statusText}`);
            }
            const data = await response.json();
            sessionId = data.id;
        }
        // Get session details
        const response = await fetch(`${REGISTRY_URL}/v1/sessions/${sessionId}`, {
            headers: {
                "Authorization": `Bearer ${TOKEN}`,
            },
        });
        if (!response.ok) {
            throw new Error(`Session not found: ${sessionId}`);
        }
        const session = await response.json();
        // Store session ID
        currentSessionId = sessionId || null;
        // Connect WebSocket for real-time messages
        connectWebSocket(sessionId);
        return {
            content: [
                {
                    type: "text",
                    text: `Attached to session: ${sessionId}\nStatus: ${session.status}\nWebSocket: ${session.websocket_url}`,
                },
            ],
        };
    }
    catch (error) {
        return {
            content: [
                {
                    type: "text",
                    text: `Error attaching: ${error instanceof Error ? error.message : String(error)}`,
                },
            ],
            isError: true,
        };
    }
}
async function handleDetach(args) {
    if (!currentSessionId) {
        return {
            content: [
                {
                    type: "text",
                    text: "Not attached to any session.",
                },
            ],
        };
    }
    const sessionId = currentSessionId;
    currentSessionId = null;
    // Close WebSocket connection
    if (wsConnection) {
        wsConnection.close();
        wsConnection = null;
        isWsConnected = false;
    }
    // Optionally terminate session
    if (args.close) {
        try {
            await fetch(`${REGISTRY_URL}/v1/sessions/${sessionId}`, {
                method: "DELETE",
                headers: {
                    "Authorization": `Bearer ${TOKEN}`,
                },
            });
        }
        catch (error) {
            // Ignore errors on close
        }
    }
    return {
        content: [
            {
                type: "text",
                text: args.close
                    ? `Detached and closed session: ${sessionId}`
                    : `Detached from session: ${sessionId}`,
            },
        ],
    };
}
async function handleStatus() {
    try {
        // List all sessions
        const response = await fetch(`${REGISTRY_URL}/v1/sessions`, {
            headers: {
                "Authorization": `Bearer ${TOKEN}`,
            },
        });
        if (!response.ok) {
            throw new Error(`Failed to fetch sessions: ${response.statusText}`);
        }
        const sessions = await response.json();
        let text = "";
        if (currentSessionId) {
            text += `Currently attached to: ${currentSessionId}\n\n`;
        }
        else {
            text += "Not attached to any session.\n\n";
        }
        text += "Available sessions:\n";
        for (const session of sessions) {
            const marker = session.id === currentSessionId ? "* " : "  ";
            text += `${marker}${session.id}  ${session.status}  ${session.created_at}\n`;
        }
        return {
            content: [
                {
                    type: "text",
                    text,
                },
            ],
        };
    }
    catch (error) {
        return {
            content: [
                {
                    type: "text",
                    text: `Error fetching status: ${error instanceof Error ? error.message : String(error)}`,
                },
            ],
            isError: true,
        };
    }
}
async function handleConfigure(args) {
    try {
        let changes = [];
        const envVars = [];
        if (args.registry_url) {
            REGISTRY_URL = args.registry_url;
            envVars.push(`INCEPTION_REGISTRY_URL=${args.registry_url}`);
            changes.push(`Registry URL: ${args.registry_url}`);
        }
        if (args.token) {
            TOKEN = args.token;
            envVars.push(`INCEPTION_TOKEN=${args.token}`);
            changes.push("Auth token: [set]");
        }
        if (changes.length === 0) {
            return {
                content: [
                    {
                        type: "text",
                        text: `Current configuration:\nRegistry URL: ${REGISTRY_URL}\nToken: ${TOKEN ? "[set]" : "[not set]"}\n\nConfig file: ${ENV_FILE}`,
                    },
                ],
            };
        }
        // Save config
        // Save to ~/.claude/channels/inception/.env
        mkdirSync(STATE_DIR, { recursive: true });
        const envContent = envVars.join("\n") + "\n";
        writeFileSync(ENV_FILE, envContent, { mode: 0o600 });
        // Test connection
        const response = await fetch(`${REGISTRY_URL}/health`, {
            method: "GET",
        });
        if (!response.ok) {
            return {
                content: [
                    {
                        type: "text",
                        text: `Configuration saved but connection test failed:\n${changes.join("\n")}\n\nError: ${response.statusText}\n\nConfig saved to: ${ENV_FILE}`,
                    },
                ],
                isError: true,
            };
        }
        return {
            content: [
                {
                    type: "text",
                    text: `Configuration saved and connection successful:\n${changes.join("\n")}\n\nConfig file: ${ENV_FILE}`,
                },
            ],
        };
    }
    catch (error) {
        return {
            content: [
                {
                    type: "text",
                    text: `Error configuring: ${error instanceof Error ? error.message : String(error)}`,
                },
            ],
            isError: true,
        };
    }
}
async function handleUpdateStatus(args) {
    try {
        if (!currentSessionId) {
            return {
                content: [
                    {
                        type: "text",
                        text: "Not attached to any session. Use inception_attach first.",
                    },
                ],
                isError: true,
            };
        }
        const updates = {};
        if (args.status)
            updates.status = args.status;
        if (args.agent_state)
            updates.agent_state = args.agent_state;
        if (args.progress !== undefined)
            updates.progress = args.progress;
        // Update status via API
        const response = await fetch(`${REGISTRY_URL}/v1/sessions/${currentSessionId}/status`, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                "Authorization": `Bearer ${TOKEN}`,
            },
            body: JSON.stringify(updates),
        });
        if (!response.ok) {
            throw new Error(`Failed to update status: ${response.statusText}`);
        }
        // Update current task if provided
        if (args.current_task) {
            const taskResponse = await fetch(`${REGISTRY_URL}/v1/sessions/${currentSessionId}`, {
                method: "PATCH",
                headers: {
                    "Content-Type": "application/json",
                    "Authorization": `Bearer ${TOKEN}`,
                },
                body: JSON.stringify({ current_task: args.current_task }),
            });
            if (!taskResponse.ok) {
                throw new Error(`Failed to update task: ${taskResponse.statusText}`);
            }
        }
        const session = await response.json();
        return {
            content: [
                {
                    type: "text",
                    text: `Status updated for ${currentSessionId}\nStatus: ${session.status}\nAgent State: ${session.agent_state || "idle"}\nProgress: ${session.progress !== null ? (session.progress * 100).toFixed(0) + "%" : "N/A"}\nTask: ${session.current_task || "None"}`,
                },
            ],
        };
    }
    catch (error) {
        return {
            content: [
                {
                    type: "text",
                    text: `Error updating status: ${error instanceof Error ? error.message : String(error)}`,
                },
            ],
            isError: true,
        };
    }
}
async function handleMetrics(args) {
    try {
        if (!currentSessionId) {
            return {
                content: [
                    {
                        type: "text",
                        text: "Not attached to any session. Use inception_attach first.",
                    },
                ],
                isError: true,
            };
        }
        const format = args.format || "summary";
        const now = new Date();
        const sessionDuration = Math.floor((now.getTime() - sessionMetrics.sessionStartTime.getTime()) / 1000);
        const idleTime = Math.floor((now.getTime() - sessionMetrics.lastActivityTime.getTime()) / 1000);
        if (format === "json") {
            return {
                content: [
                    {
                        type: "text",
                        text: JSON.stringify({
                            sessionId: currentSessionId,
                            duration: sessionDuration,
                            idleTime,
                            ...sessionMetrics,
                            sessionStartTime: sessionMetrics.sessionStartTime.toISOString(),
                            lastActivityTime: sessionMetrics.lastActivityTime.toISOString(),
                            activities: sessionMetrics.activities.map(a => ({
                                ...a,
                                startTime: a.startTime.toISOString(),
                                endTime: a.endTime?.toISOString(),
                            })),
                        }, null, 2),
                    },
                ],
            };
        }
        const recentActivities = sessionMetrics.activities.slice(-10).reverse();
        const activityList = recentActivities.map(a => {
            const duration = a.endTime
                ? ` (${Math.floor((a.endTime.getTime() - a.startTime.getTime()) / 1000)}s)`
                : "";
            const status = a.success ? "✓" : "✗";
            return `  ${status} ${a.toolName}${duration}`;
        }).join("\n");
        const summary = `Session Metrics for ${currentSessionId}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📊 Overview
  Duration: ${formatDuration(sessionDuration)}
  Idle: ${formatDuration(idleTime)}

🔧 Activity
  Total Tools: ${sessionMetrics.totalToolsUsed}
  Files Edited: ${sessionMetrics.totalFilesEdited}
  Commands Run: ${sessionMetrics.totalCommandsRun}

📝 Recent Activity (last 10)
${activityList || "  No activity yet"}

${sessionMetrics.currentActivity
            ? `⏳ Current: ${sessionMetrics.currentActivity.toolName} (started ${formatDuration(Math.floor((now.getTime() - sessionMetrics.currentActivity.startTime.getTime()) / 1000))} ago)`
            : "⏳ Current: Idle"}
`;
        if (format === "detailed") {
            const detailedActivities = sessionMetrics.activities.slice(-20).reverse().map((a, i) => {
                const duration = a.endTime
                    ? `${Math.floor((a.endTime.getTime() - a.startTime.getTime()) / 1000)}s`
                    : "ongoing";
                const input = JSON.stringify(a.toolInput).substring(0, 100);
                return `
[${sessionMetrics.activities.length - i}] ${a.toolName} (${duration})
    Input: ${input}${input.length >= 100 ? "..." : ""}
    Status: ${a.success ? "success" : "failed"}${a.error ? `\n    Error: ${a.error}` : ""}`;
            }).join("\n");
            return {
                content: [
                    {
                        type: "text",
                        text: summary + "\n📋 Detailed Activity (last 20)" + detailedActivities,
                    },
                ],
            };
        }
        return {
            content: [
                {
                    type: "text",
                    text: summary,
                },
            ],
        };
    }
    catch (error) {
        return {
            content: [
                {
                    type: "text",
                    text: `Error getting metrics: ${error instanceof Error ? error.message : String(error)}`,
                },
            ],
            isError: true,
        };
    }
}
function formatDuration(seconds) {
    if (seconds < 60)
        return `${seconds}s`;
    if (seconds < 3600)
        return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
    return `${Math.floor(seconds / 3600)}h ${Math.floor((seconds % 3600) / 60)}m`;
}
async function handleVerdict(args) {
    try {
        await handlePermissionVerdict(args.request_id, args.decision === "allow");
        return {
            content: [
                {
                    type: "text",
                    text: `Permission ${args.request_id} ${args.decision}ed`,
                },
            ],
        };
    }
    catch (error) {
        return {
            content: [
                {
                    type: "text",
                    text: `Error: ${error instanceof Error ? error.message : String(error)}`,
                },
            ],
            isError: true,
        };
    }
}
async function handleReply(args) {
    try {
        if (!currentSessionId) {
            return {
                content: [
                    {
                        type: "text",
                        text: "Not attached to any session. Use inception_attach first.",
                    },
                ],
                isError: true,
            };
        }
        const response = {
            id: `resp-${Date.now()}`,
            content: args.content,
            in_reply_to: args.reply_to,
            timestamp: new Date().toISOString(),
            source: "claude_code",
        };
        // Try WebSocket first
        if (sendMessageViaWebSocket(response)) {
            return {
                content: [
                    {
                        type: "text",
                        text: `Reply sent via WebSocket`,
                    },
                ],
            };
        }
        // Fallback to HTTP API
        const httpResponse = await fetch(`${REGISTRY_URL}/v1/sessions/${currentSessionId}/messages`, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                "Authorization": `Bearer ${TOKEN}`,
            },
            body: JSON.stringify(response),
        });
        if (!httpResponse.ok) {
            throw new Error(`Failed to send reply: ${httpResponse.statusText}`);
        }
        return {
            content: [
                {
                    type: "text",
                    text: `Reply sent via HTTP API`,
                },
            ],
        };
    }
    catch (error) {
        return {
            content: [
                {
                    type: "text",
                    text: `Error sending reply: ${error instanceof Error ? error.message : String(error)}`,
                },
            ],
            isError: true,
        };
    }
}
// Hook handlers
async function handleHook(event, data) {
    if (!currentSessionId)
        return;
    const now = new Date();
    sessionMetrics.lastActivityTime = now;
    switch (event) {
        case "SessionStart":
            sessionMetrics.sessionStartTime = now;
            sessionMetrics.activities = [];
            await updateRegistryStatus({
                status: "busy",
                agent_state: "thinking",
                current_task: "Session started",
            });
            break;
        case "UserPromptSubmit":
            await updateRegistryStatus({
                agent_state: "thinking",
                current_task: data.prompt?.substring(0, 100) || "Processing user input",
            });
            break;
        case "PreToolUse":
            sessionMetrics.totalToolsUsed++;
            if (data.tool_name === "Edit" || data.tool_name === "Write") {
                sessionMetrics.totalFilesEdited++;
            }
            if (data.tool_name === "Bash") {
                sessionMetrics.totalCommandsRun++;
            }
            const activity = {
                toolName: data.tool_name,
                toolInput: data.tool_input,
                startTime: now,
                success: true,
            };
            sessionMetrics.currentActivity = activity;
            sessionMetrics.activities.push(activity);
            // Keep only last 50 activities
            if (sessionMetrics.activities.length > 50) {
                sessionMetrics.activities = sessionMetrics.activities.slice(-50);
            }
            await updateRegistryStatus({
                agent_state: "executing",
                current_task: `${data.tool_name}: ${getActivityDescription(data)}`,
            });
            break;
        case "PostToolUse":
            if (sessionMetrics.currentActivity) {
                sessionMetrics.currentActivity.endTime = now;
                sessionMetrics.currentActivity.success = true;
            }
            await updateRegistryStatus({
                agent_state: "thinking",
            });
            break;
        case "PostToolUseFailure":
            if (sessionMetrics.currentActivity) {
                sessionMetrics.currentActivity.endTime = now;
                sessionMetrics.currentActivity.success = false;
                sessionMetrics.currentActivity.error = data.error;
            }
            await updateRegistryStatus({
                agent_state: "error",
                current_task: `Error in ${data.tool_name}: ${data.error?.substring(0, 100)}`,
            });
            break;
        case "PermissionRequest":
            await updateRegistryStatus({
                agent_state: "waiting_for_user",
                current_task: `Waiting for permission: ${data.tool_name}`,
            });
            break;
        case "Notification":
            if (data.notification_type === "idle_prompt") {
                await updateRegistryStatus({
                    agent_state: "waiting_for_user",
                    current_task: "Waiting for user input",
                });
            }
            break;
        case "SubagentStart":
            await updateRegistryStatus({
                agent_state: "executing",
                current_task: `Subagent: ${data.agent_type}`,
            });
            break;
        case "SubagentStop":
            await updateRegistryStatus({
                agent_state: "thinking",
            });
            break;
        case "Stop":
            await updateRegistryStatus({
                agent_state: "idle",
                current_task: "Ready for next task",
            });
            break;
        case "StopFailure":
            await updateRegistryStatus({
                agent_state: "error",
                current_task: `Error: ${data.error_type}`,
            });
            break;
        case "PreCompact":
            await updateRegistryStatus({
                agent_state: "thinking",
                current_task: "Compacting context...",
            });
            break;
        case "PostCompact":
            await updateRegistryStatus({
                agent_state: "idle",
            });
            break;
        case "SessionEnd":
            await updateRegistryStatus({
                status: "idle",
                agent_state: "idle",
                current_task: "Session ended",
            });
            break;
        case "TaskCompleted":
            await updateRegistryStatus({
                agent_state: "idle",
                current_task: "Task completed",
            });
            break;
    }
}
function getActivityDescription(data) {
    const toolName = data.tool_name;
    const input = data.tool_input || {};
    switch (toolName) {
        case "Bash":
            return input.command?.substring(0, 50) || "Running command";
        case "Edit":
        case "Write":
            return input.file_path || "Editing file";
        case "Read":
            return `Reading ${input.file_path || "file"}`;
        case "Glob":
            return `Searching ${input.pattern || "files"}`;
        case "Grep":
            return `Finding "${input.pattern?.substring(0, 30) || "pattern"}"`;
        case "mcp__inception__inception_attach":
            return "Attaching to Inception session";
        case "mcp__inception__inception_detach":
            return "Detaching from Inception session";
        default:
            if (toolName.startsWith("mcp__")) {
                return `MCP: ${toolName.split("__").pop()}`;
            }
            return toolName;
    }
}
async function updateRegistryStatus(updates) {
    if (!currentSessionId)
        return;
    try {
        const statusUpdate = {};
        if (updates.status)
            statusUpdate.status = updates.status;
        if (updates.agent_state)
            statusUpdate.agent_state = updates.agent_state;
        if (updates.progress !== undefined)
            statusUpdate.progress = updates.progress;
        await fetch(`${REGISTRY_URL}/v1/sessions/${currentSessionId}/status`, {
            method: "POST",
            headers: {
                "Content-Type": "application/json",
                "Authorization": `Bearer ${TOKEN}`,
            },
            body: JSON.stringify(statusUpdate),
        });
        if (updates.current_task) {
            await fetch(`${REGISTRY_URL}/v1/sessions/${currentSessionId}`, {
                method: "PATCH",
                headers: {
                    "Content-Type": "application/json",
                    "Authorization": `Bearer ${TOKEN}`,
                },
                body: JSON.stringify({ current_task: updates.current_task }),
            });
        }
    }
    catch (error) {
        console.error("Failed to update registry status:", error);
    }
}
// WebSocket connection to registry
function connectWebSocket(sessionId) {
    if (wsConnection?.readyState === WebSocket.OPEN) {
        return; // Already connected
    }
    const wsUrl = `${REGISTRY_URL.replace("http://", "ws://").replace("https://", "wss://")}/v1/sessions/${sessionId}/ws`;
    console.error(`Connecting WebSocket to ${wsUrl}`);
    wsConnection = new WebSocket(wsUrl, {
        headers: TOKEN ? { Authorization: `Bearer ${TOKEN}` } : undefined,
    });
    wsConnection.on("open", () => {
        console.error("WebSocket connected to registry");
        isWsConnected = true;
        // Send any queued messages
        while (messageQueue.length > 0) {
            const msg = messageQueue.shift();
            wsConnection?.send(JSON.stringify(msg));
        }
    });
    wsConnection.on("message", (data) => {
        try {
            const msg = JSON.parse(data.toString());
            console.error("Received message from registry:", msg);
            // Forward to Claude Code via MCP notification
            console.error("Forwarding to Claude via MCP notification...");
            server.notification({
                method: "notifications/claude/channel",
                params: {
                    content: msg.content || "(no content)",
                    meta: {
                        session_id: currentSessionId,
                        message_id: msg.id,
                        source: msg.source || "registry",
                        timestamp: msg.timestamp,
                        ...(msg.in_reply_to ? { in_reply_to: msg.in_reply_to } : {}),
                    },
                },
            }).then(() => {
                console.error("Successfully forwarded message to Claude");
            }).catch((err) => {
                console.error("Failed to deliver message to Claude:", err);
            });
        }
        catch (error) {
            console.error("Failed to parse WebSocket message:", error);
        }
    });
    wsConnection.on("close", () => {
        console.error("WebSocket disconnected");
        isWsConnected = false;
        wsConnection = null;
        // Attempt to reconnect after delay
        setTimeout(() => {
            if (currentSessionId) {
                connectWebSocket(currentSessionId);
            }
        }, 5000);
    });
    wsConnection.on("error", (error) => {
        console.error("WebSocket error:", error);
    });
}
function sendMessageViaWebSocket(msg) {
    if (wsConnection?.readyState === WebSocket.OPEN) {
        wsConnection.send(JSON.stringify(msg));
        return true;
    }
    // Queue for later
    messageQueue.push(msg);
    return false;
}
// HTTP hook server
function startHookServer() {
    const server = http.createServer(async (req, res) => {
        if (req.method !== "POST") {
            res.writeHead(405);
            res.end("Method not allowed");
            return;
        }
        let body = "";
        req.on("data", (chunk) => {
            body += chunk.toString();
        });
        req.on("end", async () => {
            try {
                const data = JSON.parse(body);
                const event = req.url?.replace("/hook/", "") || "unknown";
                await handleHook(event, data);
                res.writeHead(200, { "Content-Type": "application/json" });
                res.end(JSON.stringify({ success: true }));
            }
            catch (error) {
                console.error("Hook error:", error);
                res.writeHead(500);
                res.end(JSON.stringify({ error: String(error) }));
            }
        });
    });
    server.listen(HOOK_PORT, () => {
        console.error(`Inception hook server listening on port ${HOOK_PORT}`);
    });
}
// Start server
async function main() {
    // Start HTTP hook server
    startHookServer();
    // Start MCP server
    const transport = new StdioServerTransport();
    await server.connect(transport);
    console.error("Inception MCP server running on stdio");
    console.error("Channel capabilities: claude/channel enabled");
    console.error("Waiting for WebSocket connections...");
}
main().catch((error) => {
    console.error("Fatal error:", error);
    process.exit(1);
});
//# sourceMappingURL=server.js.map