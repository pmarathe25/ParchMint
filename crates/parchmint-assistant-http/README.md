# Assistant HTTP providers

Blocking, bounded adapters for OpenAI Responses, Anthropic Messages, and
OpenAI-compatible Chat Completions, called from the assistant worker. Transport is injectable for
tests, redirects are disabled, and errors never include credentials or response
bodies. OpenAI and Anthropic Research require a completed web-search call and URL citations;
compatible endpoints do not claim web search. Provider settings bind credentials
to the validated endpoint and protocol.

OpenAI Responses Ask and Research, Anthropic Ask, and opted-in compatible Ask
can provide bounded provisional text while a request runs. Only a completed
response is validated and returned; cancellation closes the streaming request
without automatically replaying it. Anthropic Research and structured actions
continue to return complete responses.

An explicitly selected original source can be downloaded as a bounded public
HTTPS PDF or text response. Redirect destinations are revalidated, DNS results
are pinned for the request, proxies and credentials are disabled, and incomplete
or unsupported responses fail without creating a project resource. DNS lookups
have a deadline and a bounded number of underlying system resolver calls, even
when the operating system cannot interrupt an individual lookup.
