---
name: money-maker
description: AI-powered autonomous strategy engine. Set a budget, let AI research opportunities, rank strategies, and create step-by-step action plans to earn money.
author: BXNode
version: "1.0.0"
category: Finance
icon: dollar-sign
---

# AI Money Maker

Set a budget and let AI research money-making opportunities, rank strategies by risk-adjusted return, and create detailed action plans.

## Inputs

### budget
- type: number
- label: Starting Budget
- default: 100
- min: 1

### currency
- type: select
- label: Currency
- default: USD
- options:
    - label: USD ($)
      value: USD
    - label: EUR
      value: EUR
    - label: GBP
      value: GBP
    - label: CNY
      value: CNY
    - label: JPY
      value: JPY

## Phases

### research
- label: Research
- button: Find Opportunities
- prompt: |
    Given a budget of ${{budget}} {{currency}}, identify 5 realistic online money-making opportunities that someone can start right away.

    For each opportunity provide:
    - name: string (short name)
    - description: string (what it involves, 2-3 sentences)
    - estimated_roi: string (e.g., "200-500% in 3 months")
    - time_to_revenue: string (e.g., "1-2 weeks")
    - risk: "low" | "medium" | "high"
    - tools_platforms: string[] (specific tools/platforms needed)

    Consider: freelancing, micro-tasks, digital products, content creation, dropshipping, affiliate marketing, online tutoring, stock photography, app/bot development, arbitrage, etc.

    Focus on opportunities that are REALISTIC for this budget level. Return ONLY a JSON array.
- output: cards
- output-fields:
    - title: name
    - body: description
    - badge: risk
    - meta:
        - label: "ROI"
          field: estimated_roi
        - label: "Time"
          field: time_to_revenue
    - tags: tools_platforms

### strategy
- label: Strategy
- button: Evaluate & Rank
- prompt: |
    Evaluate these money-making opportunities for someone with a ${{budget}} {{currency}} budget. Rank them by best risk-adjusted return.

    Opportunities:
    {{previous_results}}

    For the top 3, provide:
    - rank: number (1 = best)
    - name: string
    - reasoning: string (why this is a good choice, 2-3 sentences)
    - steps_overview: string (brief overview of what to do)
    - estimated_monthly_return: string (e.g., "$50-150/month")
    - risk: "low" | "medium" | "high"

    Return ONLY a JSON array of the top 3 strategies.
- output: cards
- output-fields:
    - title: name
    - body: reasoning
    - subtitle: steps_overview
    - badge: risk
    - meta:
        - label: "Monthly Return"
          field: estimated_monthly_return
- selectable: true
- select-prompt: Create Plan

### plan
- label: Action Plan
- button: Create Plan
- prompt: |
    Create a detailed step-by-step action plan for the following money-making strategy:

    Strategy: {{selected_item.name}}
    Overview: {{selected_item.steps_overview}}
    Budget: ${{budget}} {{currency}}

    Include:
    - step: number
    - action: string (short action name)
    - details: string (specific instructions, 2-3 sentences)
    - platform: string (tool or platform to use)
    - timeline: string (e.g., "Day 1-2")
    - expected_outcome: string (what success looks like for this step)

    Provide 8-12 actionable steps that go from setup to first revenue. Be specific about platforms, tools, and exact actions.
    Return ONLY a JSON array.
- output: checklist
- output-fields:
    - title: action
    - body: details
    - meta:
        - label: "Platform"
          field: platform
        - label: "Timeline"
          field: timeline
        - label: "Outcome"
          field: expected_outcome

### adapt
- label: Adapt
- button: Adapt Plan
- requires-notes: true
- prompt: |
    I'm executing this money-making strategy: {{selected_item.name}}
    Budget: ${{budget}} {{currency}}

    Completed steps: {{completed_steps}}
    Remaining steps: {{remaining_steps}}
    {{#if notes}}My notes/results so far: {{notes}}{{/if}}

    Based on the progress, suggest any adjustments to the remaining steps. Return the updated remaining steps as a JSON array with fields: step, action, details, platform, timeline, expected_outcome.
    Return ONLY the JSON array.
- output: checklist
- output-fields:
    - title: action
    - body: details
    - meta:
        - label: "Platform"
          field: platform
        - label: "Timeline"
          field: timeline
        - label: "Outcome"
          field: expected_outcome
