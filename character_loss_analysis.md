# Character Loss Analysis - Text Wrapping Implementation

## Issue Summary
Characters are being lost when text wraps:
1. In code blocks: 'S' missing from 'OPENSSL' when "export OPENSSL_DIR" wraps
2. In markdown text: 'a' missing from 'arise' and 'f' missing from 'from'

## Root Cause Analysis

### The Problem Location
The issue is in `wrap_text_with_estimates` function, specifically how it handles wrapping at spaces.

### Current Logic Flow

When text needs to wrap and a space is found:

1. **Line 1579**: When we encounter a space, we record:
   - `last_space_pos = Some(actual_pos)` (byte position of the space)
   - `last_space_width = line_width` (width up to and including the space)

2. **Line 1566**: When wrapping at a space:
   - `wrap_pos = space_pos` (wrap AT the space, not AFTER it)

3. **Line 1587**: The wrapped line is created with:
   - `byte_end = line_byte_start + wrap_pos`
   - This means the line INCLUDES the space character

4. **Line 1600-1601**: For the next line:
   ```rust
   if wrap_pos < line_byte_len && line_text.as_bytes()[wrap_pos] == b' ' {
       current_pos = wrap_pos + 1;  // Skip the space
   }
   ```

### The Bug

The issue is on **line 1566**: When we wrap at a space, we should set `wrap_pos = space_pos + 1` to include the character AFTER the space in the wrapped line, not the space itself.

Currently:
- Line 1: "export OPENSS" (ends at the space before 'L')
- Line 2: Starts after the space, which skips the 'L'

What should happen:
- Line 1: "export OPENSSL" (ends after the 'L')
- Line 2: Starts with "DIR"

### Why This Causes Character Loss

When `wrap_pos = space_pos`:
1. The first line ends AT the space position
2. The text extraction gets: `original_text[line.byte_offset..line.byte_end]`
3. This includes text up to but NOT including the character at `line.byte_end`
4. So if space is at position 13, we get characters 0-12
5. The next line starts at position 14 (space + 1)
6. **Character at position 13 (the space) is included in neither line!**

But wait, that's not quite right either. Let me re-analyze...

Actually, the issue might be different. Let me check the actual character positions more carefully.

### Corrected Analysis

Looking at line 1576:
```rust
wrap_pos = actual_pos + ch.len_utf8();
```

This sets `wrap_pos` to the position AFTER the current character. So when we process a space:
1. `actual_pos` = position of the space
2. `wrap_pos` = position after the space

Then on line 1566:
```rust
wrap_pos = space_pos;
```

This resets `wrap_pos` back to the position OF the space, not after it.

So the wrapped line will include text up to (but not including) the space.
Then the next line starts at `wrap_pos + 1`, which is the position after the space.

**This means the character AT position `wrap_pos` (which is the space) gets skipped entirely!**

But that's still not the issue because we're losing non-space characters...

### The Real Issue

After more careful analysis, I believe the issue is with how `last_space_pos` is used:

1. When we find a space at position N, we set `last_space_pos = Some(N)`
2. Later, when we need to wrap, we set `wrap_pos = last_space_pos`
3. The wrapped line ends at position N (the space)
4. The next line starts at position N+1 (after the space)

But here's the problem: **String slicing in Rust is exclusive of the end index!**

So `&text[start..end]` includes characters from `start` up to but NOT including `end`.

When we create the wrapped line with `byte_end = line_byte_start + wrap_pos`, and then extract text with `&original_text[line.byte_offset..line.byte_end]`, we get:
- All characters up to but NOT including the character at `byte_end`
- If `byte_end` points to a space, we don't include the space
- The next line starts AFTER the space, so the space is lost

But that's still not explaining why we lose 'S' from 'OPENSSL'...

### Final Analysis - The Actual Bug

The issue is more subtle. When wrapping at a space:

1. `last_space_pos` records the position of the space
2. When we wrap, `wrap_pos = space_pos` 
3. The line ends at the space position
4. BUT - we want the line to include everything UP TO the space, not including it

The character after the space is being lost because of an off-by-one error in how positions are tracked.

Example with "export OPENSSL DIR":
- Space is at position 14
- We set wrap_pos = 14
- First line: text[0..14] = "export OPENSSL" (correct!)
- Next line should start at position 15 (after the space)
- But current_pos is set to wrap_pos + 1 = 15
- If there's any issue with leading space calculation, we might skip more

The issue might be in the interaction between `line_start_pos`, `skip_spaces`, and how the actual text is extracted.

## Recommended Investigation

1. Add detailed logging in `wrap_text_with_estimates` to track:
   - The exact byte positions when wrapping
   - The characters at those positions
   - The values of `line_start_pos`, `wrap_pos`, `current_pos`

2. Add logging in `shape_line_with_styles` to track:
   - The exact text being extracted
   - The values of `line.byte_offset`, `line.byte_end`, `line.leading_space_bytes`

3. Check if the issue is related to the interaction between `line_start_pos` and `skip_spaces` in the wrapping logic.

## Specific Line Numbers to Check

- Line 1536: `let line_start_pos = current_pos;`
- Line 1546-1548: Space skipping logic that modifies `current_pos`
- Line 1566: `wrap_pos = space_pos;`
- Line 1586-1587: Creating the WrappedLine
- Line 1600-1604: Setting `current_pos` for next iteration
- Line 1636: Text extraction in `shape_line_with_styles`
- Line 1648: Applying space skipping to extracted text