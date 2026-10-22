use std::fmt::Write as _;

use zvs_contract::validate::js_slice;

pub const CONTEXT_LINES: usize = 3;
pub const LCS_CELLS: usize = 4_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineDiff {
    pub added: u64,
    pub removed: u64,
    pub text: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

fn split(text: &str) -> Vec<&str> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    lines
}

fn middle(before: &[&str], after: &[&str], offset: (usize, usize), ops: &mut Vec<Op>) {
    let (rows, columns) = (before.len(), after.len());
    let deletes = (0..rows).map(|row| Op::Delete(offset.0 + row));
    let inserts = (0..columns).map(|column| Op::Insert(offset.1 + column));
    if rows == 0 || columns == 0 || rows.saturating_mul(columns) > LCS_CELLS {
        ops.extend(deletes);
        ops.extend(inserts);
        return;
    }
    let width = columns + 1;
    let mut table = vec![0_u32; (rows + 1) * width];
    for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            table[row * width + column] = if before[row] == after[column] {
                table[(row + 1) * width + column + 1] + 1
            } else {
                table[(row + 1) * width + column].max(table[row * width + column + 1])
            };
        }
    }
    let (mut row, mut column) = (0, 0);
    while row < rows && column < columns {
        if before[row] == after[column] {
            ops.push(Op::Equal(offset.0 + row, offset.1 + column));
            row += 1;
            column += 1;
        } else if table[(row + 1) * width + column] >= table[row * width + column + 1] {
            ops.push(Op::Delete(offset.0 + row));
            row += 1;
        } else {
            ops.push(Op::Insert(offset.1 + column));
            column += 1;
        }
    }
    ops.extend((row..rows).map(|row| Op::Delete(offset.0 + row)));
    ops.extend((column..columns).map(|column| Op::Insert(offset.1 + column)));
}

fn operations(before: &[&str], after: &[&str]) -> Vec<Op> {
    let prefix = before
        .iter()
        .zip(after)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    let mut ops: Vec<Op> = (0..prefix).map(|index| Op::Equal(index, index)).collect();
    middle(
        &before[prefix..before.len() - suffix],
        &after[prefix..after.len() - suffix],
        (prefix, prefix),
        &mut ops,
    );
    ops.extend(
        (0..suffix)
            .map(|index| Op::Equal(before.len() - suffix + index, after.len() - suffix + index)),
    );
    ops
}

fn range(start: usize, count: usize) -> String {
    let first = if count == 0 { start } else { start + 1 };
    format!("{first},{count}")
}

pub fn diff_lines(before: &str, after: &str, limit: usize) -> LineDiff {
    let old = split(before);
    let new = split(after);
    let ops = operations(&old, &new);
    let mut old_seen = Vec::with_capacity(ops.len() + 1);
    let mut new_seen = Vec::with_capacity(ops.len() + 1);
    let (mut olds, mut news) = (0, 0);
    for op in &ops {
        old_seen.push(olds);
        new_seen.push(news);
        match op {
            Op::Equal(..) => {
                olds += 1;
                news += 1;
            }
            Op::Delete(_) => olds += 1,
            Op::Insert(_) => news += 1,
        }
    }
    old_seen.push(olds);
    new_seen.push(news);
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Equal(..)))
        .map(|(index, _)| index)
        .collect();
    let added = ops.iter().filter(|op| matches!(op, Op::Insert(_))).count() as u64;
    let removed = ops.iter().filter(|op| matches!(op, Op::Delete(_))).count() as u64;
    let mut text = String::new();
    let mut position = 0;
    while position < changed.len() {
        let mut last = position;
        while last + 1 < changed.len() && changed[last + 1] - changed[last] <= 2 * CONTEXT_LINES {
            last += 1;
        }
        let start = changed[position].saturating_sub(CONTEXT_LINES);
        let end = (changed[last] + CONTEXT_LINES + 1).min(ops.len());
        let _ = writeln!(
            text,
            "@@ -{} +{} @@",
            range(old_seen[start], old_seen[end] - old_seen[start]),
            range(new_seen[start], new_seen[end] - new_seen[start]),
        );
        for op in &ops[start..end] {
            let _ = match op {
                Op::Equal(index, _) => writeln!(text, " {}", old[*index]),
                Op::Delete(index) => writeln!(text, "-{}", old[*index]),
                Op::Insert(index) => writeln!(text, "+{}", new[*index]),
            };
        }
        position = last + 1;
    }
    let sliced = js_slice(&text, limit);
    let truncated = sliced.len() < text.len();
    LineDiff {
        added,
        removed,
        text: sliced.to_owned(),
        truncated,
    }
}
