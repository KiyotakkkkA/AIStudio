# Filling PDF forms

## 1. Find out whether the form is fillable

Run `scripts/form_fields.py <file>`. It prints a JSON array, one entry per field:

```json
[
  { "name": "applicant.surname", "type": "text", "value": null, "options": [], "page": 1, "required": true, "readOnly": false, "tooltip": "Фамилия" },
  { "name": "agree", "type": "checkbox", "value": "/Off", "options": ["/Yes"], "page": 2, "required": false, "readOnly": false, "tooltip": null },
  { "name": "region", "type": "combo", "value": null, "options": ["Москва", "Санкт-Петербург"], "page": 1, "required": false, "readOnly": false, "tooltip": null }
]
```

When it prints that the document has no fillable fields, the form is a flat page. Tell the user that it cannot be filled field by field, and offer to produce a filled copy with `create_pdf.py` instead, or a watermark-style overlay if they need the original layout.

## 2. Map the user's data to field names

- Use `name` exactly as printed, including dots: dotted names belong to a field hierarchy.
- `tooltip` and `page` help to tell what an unclear name means. When the meaning of a field is still unclear, ask rather than guess.
- Skip `readOnly` fields.
- Text fields take strings. Dates go in the format the form shows next to the field, or `ДД.ММ.ГГГГ` for Russian forms.
- Checkboxes take `true`/`false`, or one of `options` (for example `"/Yes"`); `false` clears the box.
- Radio groups and combo boxes take one value from `options`.

## 3. Fill

Write the values as a JSON object and pass it on stdin:

```
fill_form.py <form.pdf> - <filled.pdf>
stdin: {"applicant.surname": "Иванова", "agree": true, "region": "Москва"}
```

Add `--flatten` only when the user wants a final, non-editable copy. A form whose field font has no Cyrillic glyphs may show Russian text incorrectly once flattened; when the fill prints a warning about unsupported characters, keep the copy editable and tell the user.

`fill_form.py` refuses names it does not know and lists them, so a typo never silently drops a value.

## 4. Check

Run `form_fields.py` on the output and compare every value with what you meant to write. Report fields you left empty, required fields especially.
