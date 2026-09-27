import { startLogin } from './interceptors';

describe('startLogin', () => {
  function memory(): Pick<Storage, 'getItem' | 'setItem'> {
    const data = new Map<string, string>();
    return { getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v) };
  }

  it('reloads once and stops when the login did not help', () => {
    const storage = memory();
    let reloads = 0;
    const reload = () => reloads++;
    expect(startLogin(storage, reload, 1_000_000)).toBe(true);
    expect(startLogin(storage, reload, 1_010_000)).toBe(false);
    expect(reloads).toBe(1);
    // Later (a new session expired) it redirects again.
    expect(startLogin(storage, reload, 1_100_000)).toBe(true);
    expect(reloads).toBe(2);
  });
});
