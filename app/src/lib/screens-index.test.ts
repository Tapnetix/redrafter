import { describe, expect, it, vi } from 'vitest';
import { SCREENS, SECTION_TITLES, screenById } from './screens-index';

describe('screens-index registry', () => {
  it('lists every settings section in wireframe order', () => {
    expect(SCREENS.map((s) => s.id)).toEqual([
      'general',
      'connections',
      'models',
      'behavior',
      'presets',
      'history',
    ]);
  });

  it('includes the Phase C screens (Presets, History) as reachable sections', () => {
    const ids = SCREENS.map((s) => s.id);
    expect(ids).toContain('presets');
    expect(ids).toContain('history');
  });

  it('gives every section a component and a title', () => {
    for (const screen of SCREENS) {
      expect(typeof screen.Component).toBe('function');
      expect(screen.title.length).toBeGreaterThan(0);
    }
  });

  it('derives SECTION_TITLES from the registry', () => {
    expect(SECTION_TITLES.presets).toBe('Presets');
    expect(SECTION_TITLES.history).toBe('History');
    expect(Object.keys(SECTION_TITLES).sort()).toEqual(SCREENS.map((s) => s.id).sort());
  });

  it('only Connections and Models carry a cross-link props factory', () => {
    const withProps = SCREENS.filter((s) => s.props).map((s) => s.id);
    expect(withProps.sort()).toEqual(['connections', 'models']);
  });

  it("Connections' props factory cross-links to the models section", () => {
    const navigate = vi.fn();
    const connections = screenById('connections');
    const props = connections.props!(navigate) as { onNavigateToModels: () => void };
    props.onNavigateToModels();
    expect(navigate).toHaveBeenCalledWith('models');
  });

  it("Models' props factory cross-links to the connections section", () => {
    const navigate = vi.fn();
    const models = screenById('models');
    const props = models.props!(navigate) as { onNavigateToConnections: () => void };
    props.onNavigateToConnections();
    expect(navigate).toHaveBeenCalledWith('connections');
  });

  it('screenById throws for an unknown id', () => {
    // @ts-expect-error -- exercising the runtime guard with an off-union id
    expect(() => screenById('nope')).toThrow(/unknown screen id/);
  });
});
