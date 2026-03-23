#!/usr/bin/env node

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  Tool,
} from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";

// Configuration from environment
let REGISTRY_URL = process.env.INCEPTION_REGISTRY_URL || "http://localhost:8080";
let TOKEN = process.env.INCEPTION_TOKEN || "";

// State
let currentSessionId: string | null = null;

// Tool definitions
const ATTACH_TOOL: Tool = {
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

const DETACH_TOOL: Tool = {
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

const STATUS_TOOL: Tool = {
  name: "inception_status",
  description: "Show current attachment status and available sessions",
  inputSchema: {
    type: "object",
  },
};

const CONFIGURE_TOOL: Tool = {
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

// Create server
const server = new Server({
  name: "inception",
  version: "0.1.0",
});

// List available tools
server.setRequestHandler(ListToolsRequestSchema, async () => {
  return {
    tools: [ATTACH_TOOL, DETACH_TOOL, STATUS_TOOL, CONFIGURE_TOOL],
  };
});

// Handle tool calls
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args } = request.params;

  switch (name) {
    case "inception_attach":
      return handleAttach(args as { session_id?: string });

    case "inception_detach":
      return handleDetach(args as { close?: boolean });

    case "inception_status":
      return handleStatus();

    case "inception_configure":
      return handleConfigure(args as { registry_url?: string; token?: string });

    default:
      throw new Error(`Unknown tool: ${name}`);
  }
});

async function handleAttach(args: { session_id?: string }) {
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

    return {
      content: [
        {
          type: "text",
          text: `Attached to session: ${sessionId}\nStatus: ${session.status}\nWebSocket: ${session.websocket_url}`,
        },
      ],
    };
  } catch (error) {
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

async function handleDetach(args: { close?: boolean }) {
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

  // Optionally terminate session
  if (args.close) {
    try {
      await fetch(`${REGISTRY_URL}/v1/sessions/${sessionId}`, {
        method: "DELETE",
        headers: {
          "Authorization": `Bearer ${TOKEN}`,
        },
      });
    } catch (error) {
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
    } else {
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
  } catch (error) {
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

async function handleConfigure(args: { registry_url?: string; token?: string }) {
  try {
    let changes: string[] = [];

    if (args.registry_url) {
      REGISTRY_URL = args.registry_url;
      changes.push(`Registry URL: ${args.registry_url}`);
    }

    if (args.token) {
      TOKEN = args.token;
      changes.push("Auth token: [set]");
    }

    if (changes.length === 0) {
      return {
        content: [
          {
            type: "text",
            text: `Current configuration:\nRegistry URL: ${REGISTRY_URL}\nToken: ${TOKEN ? "[set]" : "[not set]"}`,
          },
        ],
      };
    }

    // Test connection
    const response = await fetch(`${REGISTRY_URL}/health`, {
      method: "GET",
    });

    if (!response.ok) {
      return {
        content: [
          {
            type: "text",
            text: `Configuration updated but connection test failed:\n${changes.join("\n")}\n\nError: ${response.statusText}`,
          },
        ],
        isError: true,
      };
    }

    return {
      content: [
        {
          type: "text",
          text: `Configuration updated and connection successful:\n${changes.join("\n")}`,
        },
      ],
    };
  } catch (error) {
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

// Start server
async function main() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("Inception MCP server running on stdio");
}

main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});
