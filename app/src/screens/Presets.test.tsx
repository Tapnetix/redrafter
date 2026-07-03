import { describe, expect, it, vi, beforeEach } from 'vitest';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import Presets from './Presets';
import * as ipc from '@/lib/ipc';
import type { Preset, ModelsListResult } from '@/lib/ipc';

vi.mock('@/lib/ipc', () => ({
  presetList: vi.fn(),
  presetSave: vi.fn(),
  presetDelete: vi.fn(),
  presetDuplicate: vi.fn(),
  presetResetDefault: vi.fn(),
  presetImport: vi.fn(),
  presetExport: vi.fn(),
  modelsList: vi.fn(),
}));

const mockedIpc = vi.mocked(ipc);

function preset(overrides: Partial<Preset> = {}): Preset {
  return {
    trigger: 'formal',
    direction: 'Rewrite formally and professionally; no slang or emoji; keep meaning.',
    model: null,
    lang: null,
    inject: null,
    examples: [],
    builtin: true,
    overridden: false,
    ...overrides,
  };
}

const BUILTINS: Preset[] = [
  preset({ trigger: 'formal' }),
  preset({ trigger: 'concise', direction: 'Tighten: cut filler, shorten, keep the point.' }),
  preset({ trigger: 'friendly', direction: 'Warmer, more personable tone; keep it natural.' }),
  preset({ trigger: 'bullets', direction: 'Restructure into clear, scannable bullet points.' }),
  preset({ trigger: 'reply', direction: 'Draft a reply to the quoted message.' }),
];

const EMPTY_MODELS: ModelsListResult = {
  models: [],
  hasActive: false,
  activeUnavailable: false,
  staleActiveModelId: null,
};

describe('Presets', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    mockedIpc.modelsList.mockResolvedValue(EMPTY_MODELS);
  });

  it('lists the built-ins under "Built-in" and user presets under "My presets"', async () => {
    mockedIpc.presetList.mockResolvedValue([
      ...BUILTINS,
      preset({
        trigger: 'standup',
        direction: 'Yesterday / Today / Blockers.',
        builtin: false,
        overridden: false,
      }),
    ]);

    render(<Presets />);

    await screen.findByTestId('preset-item-formal');
    expect(screen.getByTestId('preset-group-builtin')).toContainElement(screen.getByTestId('preset-item-formal'));
    expect(screen.getByTestId('preset-group-mine')).toContainElement(screen.getByTestId('preset-item-standup'));
  });

  it('selecting a preset loads its fields into the editor', async () => {
    mockedIpc.presetList.mockResolvedValue([
      preset({
        trigger: 'reply-de',
        direction: 'Reply in German, warm but concise.',
        model: 'claude-sonnet-4-6',
        lang: 'de',
        inject: 'review',
        builtin: false,
        overridden: false,
        examples: [{ before: 'thanks', after: 'Danke' }],
      }),
    ]);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-reply-de'));

    expect(screen.getByTestId('preset-name')).toHaveValue('/reply-de');
    expect(screen.getByTestId('preset-direction')).toHaveValue('Reply in German, warm but concise.');
    expect(screen.getByTestId('preset-model')).toHaveValue('claude-sonnet-4-6');
    expect(screen.getByTestId('preset-lang')).toHaveValue('de');
    expect(screen.getByTestId('preset-inject')).toHaveValue('review');
    expect(screen.getByTestId('preset-example-before-1')).toHaveValue('thanks');
    expect(screen.getByTestId('preset-example-after-1')).toHaveValue('Danke');
  });

  it('preset-new clears the editor for a fresh preset', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-formal'));
    expect(screen.getByTestId('preset-name')).toHaveValue('/formal');

    fireEvent.click(screen.getByTestId('preset-new'));

    expect(screen.getByTestId('preset-name')).toHaveValue('');
    expect(screen.getByTestId('preset-direction')).toHaveValue('');
    expect(screen.queryByTestId('preset-builtin-badge')).not.toBeInTheDocument();
  });

  it('creating and saving a brand-new preset calls preset_save directly (no override confirm)', async () => {
    mockedIpc.presetList.mockResolvedValueOnce(BUILTINS).mockResolvedValueOnce([
      ...BUILTINS,
      preset({ trigger: 'standup', direction: 'Yesterday / Today / Blockers.', builtin: false }),
    ]);
    mockedIpc.presetSave.mockResolvedValue(
      preset({ trigger: 'standup', direction: 'Yesterday / Today / Blockers.', builtin: false }),
    );

    render(<Presets />);
    await screen.findByTestId('preset-item-formal');

    fireEvent.click(screen.getByTestId('preset-new'));
    fireEvent.change(screen.getByTestId('preset-name'), { target: { value: '/standup' } });
    fireEvent.change(screen.getByTestId('preset-direction'), {
      target: { value: 'Yesterday / Today / Blockers.' },
    });
    fireEvent.click(screen.getByTestId('preset-save'));

    await waitFor(() =>
      expect(mockedIpc.presetSave).toHaveBeenCalledWith(
        expect.objectContaining({ trigger: 'standup', direction: 'Yesterday / Today / Blockers.' }),
      ),
    );
    expect(screen.queryByTestId('preset-override-modal')).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId('preset-item-standup')).toBeInTheDocument());
  });

  it('saving over a built-in trigger opens the override-confirm dialog before calling preset_save', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-formal'));
    fireEvent.change(screen.getByTestId('preset-direction'), { target: { value: 'Slightly less formal.' } });
    fireEvent.click(screen.getByTestId('preset-save'));

    expect(await screen.findByTestId('preset-override-modal')).toBeVisible();
    expect(mockedIpc.presetSave).not.toHaveBeenCalled();

    mockedIpc.presetSave.mockResolvedValue(
      preset({ trigger: 'formal', direction: 'Slightly less formal.', overridden: true }),
    );
    fireEvent.click(screen.getByTestId('preset-override-confirm'));

    await waitFor(() =>
      expect(mockedIpc.presetSave).toHaveBeenCalledWith(
        expect.objectContaining({ trigger: 'formal', direction: 'Slightly less formal.' }),
      ),
    );
  });

  it('cancelling the override-confirm dialog does not call preset_save', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-formal'));
    fireEvent.change(screen.getByTestId('preset-direction'), { target: { value: 'Slightly less formal.' } });
    fireEvent.click(screen.getByTestId('preset-save'));

    fireEvent.click(await screen.findByTestId('preset-override-cancel'));

    expect(screen.queryByTestId('preset-override-modal')).not.toBeInTheDocument();
    expect(mockedIpc.presetSave).not.toHaveBeenCalled();
  });

  it('an empty trigger shows an error instead of saving', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);

    render(<Presets />);
    await screen.findByTestId('preset-item-formal');
    fireEvent.click(screen.getByTestId('preset-new'));
    fireEvent.click(screen.getByTestId('preset-save'));

    expect(await screen.findByTestId('preset-save-error')).toBeInTheDocument();
    expect(mockedIpc.presetSave).not.toHaveBeenCalled();
  });

  it('the built-in warning offers duplicate + reset-to-default for a built-in preset', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);
    mockedIpc.presetDuplicate.mockResolvedValue(
      preset({ trigger: 'formal-copy', builtin: false, overridden: false }),
    );

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-formal'));

    expect(screen.getByTestId('preset-builtin-warning')).toBeVisible();
    fireEvent.click(screen.getByTestId('preset-duplicate'));

    await waitFor(() => expect(mockedIpc.presetDuplicate).toHaveBeenCalledWith('formal', 'formal-copy'));
  });

  it('reset-to-default calls preset_reset_default for an overridden built-in', async () => {
    mockedIpc.presetList
      .mockResolvedValueOnce([preset({ trigger: 'formal', direction: 'custom', overridden: true })])
      .mockResolvedValueOnce(BUILTINS);
    mockedIpc.presetResetDefault.mockResolvedValue(undefined);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-formal'));
    fireEvent.click(screen.getByTestId('preset-reset-default'));

    await waitFor(() => expect(mockedIpc.presetResetDefault).toHaveBeenCalledWith('formal'));
  });

  it('deleting a preset opens a confirm dialog, and confirming calls preset_delete', async () => {
    mockedIpc.presetList.mockResolvedValueOnce([
      ...BUILTINS,
      preset({ trigger: 'standup', direction: 'x', builtin: false }),
    ]);
    mockedIpc.presetDelete.mockResolvedValue(undefined);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-standup'));
    fireEvent.click(screen.getByTestId('preset-delete'));

    expect(await screen.findByTestId('delete-preset-modal')).toBeVisible();
    fireEvent.click(screen.getByTestId('preset-delete-cancel'));
    expect(screen.queryByTestId('delete-preset-modal')).not.toBeInTheDocument();
    expect(mockedIpc.presetDelete).not.toHaveBeenCalled();

    mockedIpc.presetList.mockResolvedValueOnce(BUILTINS);
    fireEvent.click(screen.getByTestId('preset-delete'));
    fireEvent.click(await screen.findByTestId('preset-delete-confirm'));

    await waitFor(() => expect(mockedIpc.presetDelete).toHaveBeenCalledWith('standup'));
  });

  it('adding and removing a few-shot example updates the form', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-formal'));
    fireEvent.click(screen.getByTestId('preset-add-example'));

    expect(screen.getByTestId('preset-example-before-1')).toBeInTheDocument();
    fireEvent.change(screen.getByTestId('preset-example-before-1'), { target: { value: 'yo' } });
    fireEvent.change(screen.getByTestId('preset-example-after-1'), { target: { value: 'Hello' } });

    fireEvent.click(screen.getByTestId('preset-example-remove-1'));
    expect(screen.queryByTestId('preset-example-before-1')).not.toBeInTheDocument();
  });

  it('cancel discards unsaved edits back to the loaded preset', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);

    render(<Presets />);
    fireEvent.click(await screen.findByTestId('preset-item-formal'));
    fireEvent.change(screen.getByTestId('preset-direction'), { target: { value: 'edited' } });
    expect(screen.getByTestId('preset-direction')).toHaveValue('edited');

    fireEvent.click(screen.getByTestId('preset-cancel'));

    expect(screen.getByTestId('preset-direction')).toHaveValue(
      'Rewrite formally and professionally; no slang or emoji; keep meaning.',
    );
  });

  it('importing pasted JSON calls preset_import and refreshes the list', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);
    mockedIpc.presetImport.mockResolvedValue({ imported: ['standup'], conflicts: [] });

    render(<Presets />);
    await screen.findByTestId('preset-item-formal');

    fireEvent.click(screen.getByTestId('preset-import'));
    fireEvent.change(screen.getByTestId('preset-import-paste'), { target: { value: '[{"trigger":"standup"}]' } });
    fireEvent.click(screen.getByTestId('preset-import-confirm'));

    await waitFor(() => expect(mockedIpc.presetImport).toHaveBeenCalledWith('[{"trigger":"standup"}]'));
    expect(await screen.findByTestId('preset-import-result')).toHaveTextContent('Imported 1 · 0 conflicts');
  });

  it('cancelling the import dialog does not call preset_import', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);

    render(<Presets />);
    await screen.findByTestId('preset-item-formal');

    fireEvent.click(screen.getByTestId('preset-import'));
    fireEvent.change(screen.getByTestId('preset-import-paste'), { target: { value: 'anything' } });
    fireEvent.click(screen.getByTestId('preset-import-cancel'));

    expect(screen.queryByTestId('import-modal')).not.toBeInTheDocument();
    expect(mockedIpc.presetImport).not.toHaveBeenCalled();
  });

  it('opening export shows the exported text, and copy re-fetches + writes to the clipboard', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);
    mockedIpc.presetExport.mockResolvedValue('[]');
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    render(<Presets />);
    await screen.findByTestId('preset-item-formal');

    fireEvent.click(screen.getByTestId('preset-export'));
    await waitFor(() => expect(screen.getByTestId('preset-export-text')).toHaveTextContent('[]'));

    fireEvent.click(screen.getByTestId('preset-export-copy'));
    await waitFor(() => expect(mockedIpc.presetExport).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('[]'));

    fireEvent.click(screen.getByTestId('preset-export-close'));
    expect(screen.queryByTestId('export-modal')).not.toBeInTheDocument();
  });

  it('populates the model-override dropdown from curated models', async () => {
    mockedIpc.presetList.mockResolvedValue(BUILTINS);
    mockedIpc.modelsList.mockResolvedValue({
      models: [
        { connectionId: '1', modelId: 'claude-opus-4-6', providerKind: 'anthropic', active: true, favorite: false },
      ],
      hasActive: true,
      activeUnavailable: false,
      staleActiveModelId: null,
    });

    render(<Presets />);
    await screen.findByTestId('preset-item-formal');

    fireEvent.click(screen.getByTestId('preset-item-formal'));
    expect(
      screen.getByRole('option', { name: 'claude-opus-4-6' }),
    ).toBeInTheDocument();
  });
});
