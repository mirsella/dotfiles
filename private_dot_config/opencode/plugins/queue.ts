import type { Plugin } from "@opencode-ai/plugin"
import type { FilePart, TextPart, TextPartInput, FilePartInput, AgentPartInput, SubtaskPartInput } from "@opencode-ai/sdk"

const prefix = /^\/queue(?:\s+([\s\S]*))?$/

type InputPart = TextPartInput | FilePartInput | AgentPartInput | SubtaskPartInput

type Info = {
  agent: string
  model: {
    providerID: string
    modelID: string
  }
}

type Item =
  | {
      kind: "prompt"
      info: Info
      parts: InputPart[]
    }
  | {
      kind: "command"
      info: Info
      command: string
      arguments: string
    }

export const QueuePlugin: Plugin = async ({ client }) => {
  const queue = new Map<string, Item[]>()
  const hidden = new Set<string>()
  const status = new Map<string, "idle" | "busy" | "retry">()
  const active = new Set<string>()

  const toast = (message: string, variant: "info" | "error") =>
    client.tui.showToast({ body: { message, variant, duration: 2500 } }).catch(() => undefined)

  const send = async (sid: string) => {
    if (active.has(sid)) return

    const list = queue.get(sid)
    if (!list?.length) return

    active.add(sid)

    try {
      while (list.length) {
        const item = list.shift()
        if (!item) return

        if (item.kind === "command") {
          await client.session.command({
            path: { id: sid },
            body: {
              agent: item.info.agent,
              model: `${item.info.model.providerID}/${item.info.model.modelID}`,
              command: item.command,
              arguments: item.arguments,
            },
          })
          continue
        }

        await client.session.prompt({
          path: { id: sid },
          body: {
            agent: item.info.agent,
            model: item.info.model,
            parts: item.parts.map((part) => ({ ...part, id: undefined })),
          },
        })
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      console.error("QueuePlugin failed to flush queued input", err)
      await toast(`Queue failed: ${msg}`, "error")
    } finally {
      if (list.length) queue.set(sid, list)
      else queue.delete(sid)
      active.delete(sid)
    }
  }

  return {
    event: async (input) => {
      if (input.event.type !== "session.status") return

      const sid = input.event.properties.sessionID
      const next = input.event.properties.status.type
      status.set(sid, next)

      if (next !== "idle") return
      await send(sid)
    },
    "chat.message": async (input, output) => {
      const text = output.parts.find((part): part is TextPart => part.type === "text" && !part.synthetic)
      if (!text) return

      const match = text.text.match(prefix)
      if (!match) return

      const body = match[1] ?? ""
      const files = output.parts.filter((part): part is FilePart => part.type === "file")
      if (!body.trim() && files.length === 0) return

      if ((status.get(input.sessionID) ?? "idle") === "idle") {
        if (body.trimStart().startsWith("/")) return
        text.text = body
        return
      }

      const parts = output.parts.flatMap((part): InputPart[] => {
        if (part.type === "text") {
          if (part.id !== text.id) return [{ ...part }]
          if (!body) return []
          return [{ ...part, text: body }]
        }
        if (part.type === "file" || part.type === "agent" || part.type === "subtask") {
          return [{ ...part }]
        }
        console.warn("QueuePlugin skipped unexpected part", part.type)
        return []
      })

      const mark = body.trim() || `${files.length} attachment${files.length === 1 ? "" : "s"}`
      const note = mark.length > 72 ? `${mark.slice(0, 69)}...` : mark
      const list = queue.get(input.sessionID) ?? []
      const info = {
        agent: output.message.agent,
        model: { ...output.message.model },
      }
      const cmd = body.trim().match(/^\/(\S+)(?:\s+([\s\S]*))?$/)

      if (cmd) {
        list.push({
          kind: "command",
          info,
          command: cmd[1],
          arguments: cmd[2] ?? "",
        })
      } else {
        list.push({
          kind: "prompt",
          info,
          parts,
        })
      }

      queue.set(input.sessionID, list)
      hidden.add(output.message.id)
      text.text = `[queued] ${note}`
      await toast(`Queued: ${note}`, "info")
    },
    "experimental.chat.messages.transform": async (_, output) => {
      output.messages = output.messages.filter((msg) => !hidden.has(msg.info.id))
    },
  }
}

export default QueuePlugin
