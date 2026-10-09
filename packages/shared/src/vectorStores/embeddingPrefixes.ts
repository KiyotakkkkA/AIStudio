export interface EmbeddingPrefixes {
  readonly queryPrefix: string;
  readonly documentPrefix: string;
}

const RETRIEVAL_INSTRUCTION = "Represent this sentence for searching relevant passages: ";

const FAMILIES: readonly { pattern: RegExp; prefixes: EmbeddingPrefixes }[] = [
  {
    pattern: /(^|[/_:-])(multilingual-)?e5([_-]|$)/i,
    prefixes: { queryPrefix: "query: ", documentPrefix: "passage: " },
  },
  {
    pattern: /nomic-embed/i,
    prefixes: { queryPrefix: "search_query: ", documentPrefix: "search_document: " },
  },
  {
    pattern: /qwen3[-_]?embedding/i,
    prefixes: {
      queryPrefix:
        "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery: ",
      documentPrefix: "",
    },
  },
  {
    pattern: /mxbai-embed-large|snowflake-arctic-embed|bge-(small|base|large)-en/i,
    prefixes: { queryPrefix: RETRIEVAL_INSTRUCTION, documentPrefix: "" },
  },
];

export const NO_EMBEDDING_PREFIXES: EmbeddingPrefixes = { queryPrefix: "", documentPrefix: "" };

export function knownEmbeddingPrefixes(modelName: string): EmbeddingPrefixes {
  return (
    FAMILIES.find((family) => family.pattern.test(modelName))?.prefixes ?? NO_EMBEDDING_PREFIXES
  );
}
