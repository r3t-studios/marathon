# AI and Machine Learning Usage Policy

## Core Principle: Human Accountability

Every contribution to Marathon must have a human who:
- **Made the decisions** about what to build and how to build it
- **Understands the code, design, or content** they're submitting
- **Takes responsibility** for the outcome and any issues that arise
- **Can be held accountable** for the contribution

AI and ML tools are welcome as assistants, but they cannot:
- Make architectural or design decisions
- Choose between technical trade-offs
- Take responsibility for bugs or issues
- Be credited as contributors

## Context: Pragmatism at a Small Scale

We're a tiny studio with limited resources. We can't afford large teams, professional translators, or extensive QA departments. **Machine learning tools help us punch above our weight class** - they let us move faster, support more languages, and catch bugs we'd otherwise miss.

We use these tools not to replace human judgment, but to stretch our small team's capacity. This is about working **smart with what we have**, not taking shortcuts that compromise quality or accountability.

We're using ethical and responsible machine learning as much as possible while ensuring that we are not erasing human contributions while we are resource-constrained.

## The Blurry Line

**Here's the honest truth:** The line between "generative AI" and "assistive AI" is fuzzy and constantly shifting. Is IDE autocomplete assistive? What about when it suggests entire functions? What about pair-programming with an LLM?

**We don't have perfect answers.** What we do have is a principle: **a human must make the decisions and be accountable.**

If you're unsure whether your use of AI crosses a line, ask yourself:
- **"Do I understand what this code does and why?"**
- **"Did I decide this was the right approach, or did the AI?"**
- **"Can I maintain and debug this?"**
- **"Am I comfortable being accountable for this?"**

If you answer "yes" to those questions, you're probably fine. If you're still uncertain, open a discussion - we'd rather have the conversation than enforce rigid rules that don't match reality.

## What This Looks Like in Practice

### Acceptable Use

**"I used Claude/Copilot to help write this function, I reviewed it, I understand it, and I'm responsible for it."**
- You directed the tool
- You reviewed and understood the output
- You made the decision to use this approach
- You take responsibility for the result

**"I directed an LLM to implement my design, then verified it meets requirements."**
- You designed the solution
- You used AI to speed up implementation
- You verified correctness
- You own the outcome

**"I used machine translation as a starting point, then reviewed and corrected the output."**
- You acknowledge the limitations of automated translation
- You applied human judgment to the result
- You ensure accuracy and appropriateness

### Not Acceptable

**"Claude wrote this, I pasted it in, seems fine."**
- No understanding of the code
- No verification of correctness
- Cannot maintain or debug
- Cannot explain design decisions

**"I asked an LLM what architecture to use and implemented its suggestion."**
- The AI made the architectural decision
- No human judgment about trade-offs
- No accountability for the choice

**"I'm submitting this AI-generated documentation without reviewing it."**
- No verification of accuracy
- No human oversight
- Cannot vouch for quality

## Why This Matters

Marathon itself was largely written with AI assistance under human direction. **That's fine!** What matters is:

1. **A human made every architectural decision**
2. **A human is accountable for every line of code**
3. **A human can explain why things work the way they do**
4. **Humans take credit AND responsibility**

Think of AI like a compiler, a library, or a really capable intern - it's a tool that amplifies human capability, but **the human is always the one making decisions and being accountable**.

## For Contributors

We don't care what tools you use to be productive. We care that:
- **You made the decisions** (not the AI)
- **You understand what you're submitting**
- **You're accountable** for the contribution
- **You can maintain it** if issues arise

Use whatever tools help you work effectively, but you must be able to answer "why did you make this choice?" with human reasoning, not "the AI suggested it."

### When Contributing

You don't need to disclose every time you use autocomplete or ask an LLM a question. We trust you to:
- Use tools responsibly
- Understand your contributions
- Take ownership of your work

If you're doing something novel or pushing boundaries with AI assistance, mentioning it in your PR is welcome - it helps us all learn and navigate this space together.

## What We Use

For transparency, here's where Marathon currently uses machine learning:

- **Development assistance** - IDE tools, code completion, pair programming with LLMs
- **Translation tooling** - Machine translation for internationalization (human-reviewed)
- **Performance analysis** - Automated profiling and optimization suggestions
- **Code review assistance** - Static analysis and potential bug detection
- **Documentation help** - Grammar checking, clarity improvements, translation

In all cases, humans review, approve, and take responsibility for the output.

## The Bottom Line

**Machines can't be held accountable, so humans must make all decisions.**

Use AI tools to help you work faster and smarter, but you must understand and be accountable for what you contribute. When in doubt, ask yourself:

**"Can a machine be blamed if this breaks?"**

If yes, you've crossed the line.

## Questions or Concerns?

This policy will evolve as we learn more about working effectively with AI tools. If you have questions, concerns, or suggestions, please open a discussion. We're figuring this out together.

---

*This policy reflects our values as of February 2026. As technology and our understanding evolve, so will this document.*
