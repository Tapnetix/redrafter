import { test, expect } from '../fixtures/setup';

/**
 * S3: Quoted context handled — given a selection containing a quoted reply
 * block (an "On <date>, <name> wrote:" header plus a `>`-quoted line) plus
 * the user's own draft, when the user refines, then the quote is used as
 * context and left unchanged while the draft is polished.
 * (design-redrafter.md S3, wireframes/capture.html, controls/capture.json)
 *
 * The real backend's `refine` command takes no arguments (it re-captures
 * the live OS selection itself, per `app/src/lib/ipc.ts`), so — as with the
 * other capture-panel specs (A9/A10/A11/A13) — the mocked `refine`
 * outcome's `original`/`refined` strings stand in for "the selection the
 * user drove through the panel" and what the backend's quote-aware
 * pipeline (`quote_parser` + `prompt_builder`, B4) produced from it. This
 * spec asserts the review-and-confirm surface (B6, `RefineOutcome.status
 * === 'pending_review'`) shows that quoted context verbatim in both the
 * Original and Refined columns (untouched) while the draft differs
 * (polished), and that accepting pastes exactly that quote-preserved,
 * draft-polished text via `inject_text` — matching `capture-refine` ->
 * `refine` and `capture-accept` -> `inject_text` from controls/capture.json.
 */

type MockCall = { cmd: string; args: Record<string, unknown> };

async function mockCalls(page: import('@playwright/test').Page): Promise<MockCall[]> {
  return page.evaluate(() => (window as unknown as { __TAURI_MOCK_CALLS__: MockCall[] }).__TAURI_MOCK_CALLS__);
}

function countOccurrences(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

const QUOTE_BLOCK = 'On Tue, Jan 6, Alex wrote:\n> Can we ship the Q3 release by Friday?';
const ROUGH_DRAFT = 'i think ya we can ship it fri, no delays i hope';
const POLISHED_DRAFT = 'Yes, we can ship it Friday — no delays.';

const ORIGINAL_SELECTION = `${QUOTE_BLOCK}\n\n${ROUGH_DRAFT}`;
const REFINED_RESULT = `${QUOTE_BLOCK}\n\n${POLISHED_DRAFT}`;

test.describe('Quoted context handled (S3)', () => {
  test.use({
    testData: {
      appName: 'redrafter',
      refineOutcome: {
        original: ORIGINAL_SELECTION,
        refined: REFINED_RESULT,
        model: 'claude-opus-4-6',
        status: 'pending_review',
      },
    },
  });

  test('S3: refining a selection with quoted context leaves the quote unchanged and polishes only the draft', async ({
    page,
  }) => {
    await page.goto('/capture');

    await expect(page.getByTestId('capture-panel')).toBeVisible();
    await page.getByTestId('capture-refine').click();

    const calls = await mockCalls(page);
    expect(calls.some((c) => c.cmd === 'refine')).toBe(true);

    // Review-and-confirm shows both the original selection and the
    // refined result so the quoted context and the draft can be compared.
    const review = page.getByTestId('capture-review');
    await expect(review).toBeVisible();

    const reviewText = (await review.textContent()) ?? '';

    // The quoted reply block appears verbatim twice — once in the
    // Original column, once in the Refined column — i.e. it was carried
    // through untouched, not rewritten.
    expect(countOccurrences(reviewText, QUOTE_BLOCK)).toBe(2);

    // The user's own words show up differently in each column: the rough
    // draft (Original) and its polished replacement (Refined) each appear
    // exactly once — the draft, not the quote, is what got refined.
    expect(countOccurrences(reviewText, ROUGH_DRAFT)).toBe(1);
    expect(countOccurrences(reviewText, POLISHED_DRAFT)).toBe(1);

    // Accepting pastes the refined result — quote intact, draft polished —
    // via inject_text (controls/capture.json's declared backend for
    // capture-accept).
    await page.getByTestId('capture-accept').click();

    const callsAfterAccept = await mockCalls(page);
    const injectCall = callsAfterAccept.find((c) => c.cmd === 'inject_text');
    expect(injectCall).toBeTruthy();
    expect(injectCall!.args.text).toBe(REFINED_RESULT);
    expect(injectCall!.args.text).toContain(QUOTE_BLOCK);
    expect(injectCall!.args.text).not.toContain(ROUGH_DRAFT);
  });
});
