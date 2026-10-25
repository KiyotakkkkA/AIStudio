# Knowledge stores

The app's knowledge stores hold documents split into fragments. They are good for facts scattered across many documents (contract amounts, incident dates, figures from reports) and poor for complete tables: search returns the most relevant fragments, not every one.

## Collect

1. `knowledge.list` shows the stores attached to the conversation and their documents.
2. `knowledge.search` with short, specific queries, one per fact type: «сумма договора», «дата подписания», «штраф за просрочку». Repeat with other wordings until new searches return nothing new.
3. `knowledge.read` the documents that matter in full when a fragment is cut off or a table spans fragments.
4. When a store holds the original files (spreadsheets, PDFs) and their paths are known, read the files themselves (see `files.md` and `documents.md`) — that is complete where search is not.

## Record

Write every extracted fact as a row into a CSV with `fs.apply`, with the columns the dashboard needs plus `document` and `fragment` (or page) for traceability:

```
contract,client,amount,signed,document
Д-114,ООО «Север»,1250000,2026-03-14,Договоры/Д-114.pdf
```

Keep amounts as numbers in one unit and currency, dates in ISO form.

## Say what is missing

A dashboard built from search results is only as complete as the search. In the report and in a footer note, say how the data was collected («по результатам поиска в хранилище «Договоры», 46 документов из 52»), and list the documents where the needed figure was not found.
