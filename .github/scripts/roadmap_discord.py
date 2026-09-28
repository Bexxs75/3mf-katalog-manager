#!/usr/bin/env python3
"""Überträgt die öffentliche GitHub-Roadmap (Projekt 1) nach Discord #roadmap.

Liest die Einträge per GraphQL und BEARBEITET zwei bestehende Webhook-
Nachrichten (DE, EN); es wird nie neu gepostet. Nur Standardbibliothek.

Umgebung:
  GH_TOKEN                 Token mit Lesezugriff auf das Projekt
  DISCORD_ROADMAP_WEBHOOK  Webhook-URL (Secret)
  DISCORD_ROADMAP_MSG_DE / _EN  IDs der beiden Nachrichten
  DRY_RUN=1                nur ausgeben, nichts an Discord senden
"""
import datetime
import json
import os
import sys
import urllib.request
import zoneinfo

OWNER, NUMBER = "Bexxs75", 1
BOARD = "https://github.com/users/Bexxs75/projects/1/views/1?groupedBy%5BcolumnId%5D=416999698"
# Sent as the description of an embed: a plain message allows only 2000
# characters, which the roadmap outgrew with five planned versions.
EMBED_LIMIT = 4096
ICON = {"Done": "✅", "In Progress": "🔧", "Todo": "▫️"}
PRIO_EN = {"Hoch": "high", "Mittel": "medium", "Niedrig": "low"}

# The version order comes from the project's "Version" field, so new versions
# show up without changing this script.
QUERY = """query($o:String!,$n:Int!){user(login:$o){projectV2(number:$n){
  field(name:"Version"){... on ProjectV2SingleSelectField{options{name}}}
  items(first:100){nodes{
    content{... on DraftIssue{title} ... on Issue{title}}
    fieldValues(first:20){nodes{... on ProjectV2ItemFieldSingleSelectValue{name field{... on ProjectV2SingleSelectField{name}}}}}
  }}
}}}"""


def fetch_items(token):
    body = json.dumps({"query": QUERY, "variables": {"o": OWNER, "n": NUMBER}}).encode()
    req = urllib.request.Request("https://api.github.com/graphql", body,
                                 {"Authorization": f"Bearer {token}", "Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=30) as r:
        data = json.load(r)
    if data.get("errors"):
        sys.exit(f"GraphQL-Fehler: {data['errors']}")
    project = (data.get("data") or {}).get("user", {}).get("projectV2")
    if not project:
        sys.exit("Projekt nicht lesbar (Token ohne Projektzugriff?)")
    versions = [o["name"] for o in (project.get("field") or {}).get("options", [])]
    items = []
    for node in project["items"]["nodes"]:
        title = (node.get("content") or {}).get("title")
        fields = {v["field"]["name"]: v["name"] for v in node["fieldValues"]["nodes"] if v and v.get("field")}
        if title and fields.get("Version") in versions:
            items.append({"title": title, **fields})
    return versions, items


def split_title(title, lang):
    de, _, en = title.partition(" / ")
    return (de if lang == "de" else (en or de)).strip()


def release_marker(version, group):
    """The "vX veröffentlicht / released" entry of a finished version, else None.

    A released version is shown as this one line: Discord allows 2000 characters
    per message, and the done items are on the board anyway.
    """
    if not group or any(i.get("Status") != "Done" for i in group):
        return None
    return next((i for i in group if i["title"].startswith(version)), None)


def render(versions, items, lang):
    today = datetime.datetime.now(zoneinfo.ZoneInfo("Europe/Berlin")).date()
    head = (f"🗺️ **Roadmap** · Stand {today:%d.%m.%Y}" if lang == "de"
            else f"🗺️ **Roadmap** · as of {today:%Y-%m-%d}")
    lines = [head]
    for version in versions:
        group = [i for i in items if i.get("Version") == version]
        if not group:
            continue
        marker = release_marker(version, group)
        if marker:
            lines.append(f"\n✅ **{split_title(marker['title'], lang)}**")
            continue
        label = version if version != "Später" else ("Später" if lang == "de" else "Later")
        lines.append(f"\n**{label}**")
        for i in group:
            text = split_title(i["title"], lang)
            if version == "Später" and i.get("Priorität"):
                prio = i["Priorität"] if lang == "de" else PRIO_EN.get(i["Priorität"], i["Priorität"])
                text += f" · _{prio}_"
            lines.append(f"{ICON.get(i.get('Status'), '▫️')} {text}")
    legend = ("✅ fertig · 🔧 in Arbeit · ▫️ geplant" if lang == "de" else "✅ done · 🔧 in progress · ▫️ planned")
    link = (f"Komplette Roadmap: <{BOARD}>" if lang == "de" else f"Full roadmap: <{BOARD}>")
    lines += ["", legend, link]
    text = "\n".join(lines)
    if len(text) > EMBED_LIMIT:
        sys.exit(f"Nachricht ({lang}) hat {len(text)} Zeichen, Discord erlaubt {EMBED_LIMIT}")
    return text


def edit(webhook, message_id, content):
    # "content": "" clears the plain text of messages written before the switch to an embed.
    body = json.dumps({"content": "", "embeds": [{"description": content}],
                       "allowed_mentions": {"parse": []}}).encode()
    req = urllib.request.Request(f"{webhook}/messages/{message_id}", body,
                                 {"Content-Type": "application/json", "User-Agent": "3mf-roadmap-sync"}, method="PATCH")
    with urllib.request.urlopen(req, timeout=30) as r:
        r.read()


def main():
    versions, items = fetch_items(os.environ["GH_TOKEN"])
    if not items:
        # Never write an empty roadmap: the GitHub Actions default token can't see the
        # items of a user project.
        sys.exit("Keine Einträge gelesen - Token ohne Projektzugriff? (Secret ROADMAP_READ_TOKEN)")
    for lang in ("de", "en"):
        text = render(versions, items, lang)
        if os.environ.get("DRY_RUN"):
            print(f"--- {lang} ({len(text)} Zeichen)\n{text}\n")
        else:
            edit(os.environ["DISCORD_ROADMAP_WEBHOOK"], os.environ[f"DISCORD_ROADMAP_MSG_{lang.upper()}"], text)
            print(f"{lang}: aktualisiert ({len(text)} Zeichen)")


if __name__ == "__main__":
    main()
