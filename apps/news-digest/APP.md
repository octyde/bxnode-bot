---
name: news-digest
description: Find and summarize news based on your interests using AI
author: BXNode
version: "1.0.0"
category: Productivity
icon: newspaper
---

# News Digest

An AI-powered news aggregator that finds and summarizes the latest news on topics you care about.

## Inputs

### topics
- type: text
- label: Topics (comma-separated)
- default: AI, IOT
- placeholder: AI, IOT, Blockchain...

## Phases

### fetch
- label: Fetch News
- button: Refresh
- prompt: |
    Today is {{date}}. Find and summarize the most recent and current news about the following topics: {{topics}}.

    IMPORTANT: Only include news from {{month_year}} or the most recent weeks. Do NOT include outdated news from previous years.

    Return your response as a JSON array of objects with these fields:
    - title: string (the article/news headline)
    - summary: string (2-3 sentence summary)
    - source: string (source name or URL if known)
    - topic: string (which topic this relates to)
    - date: string (the publication date of the news, e.g. "March 12, 2026")

    Provide 3-5 articles per topic. Keep summaries concise and informative.
    Return ONLY the JSON array, no other text.
- output: cards
- output-fields:
    - title: title
    - body: summary
    - badge: topic
    - footer-left: source
    - footer-right: date
