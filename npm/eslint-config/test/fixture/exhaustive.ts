type T = 'a' | 'b';

export function f(v: T): number {
  switch (v) {
    case 'a':
      return 1;
    case 'b':
      return 2;
    default: {
      const unreachable: never = v;
      return unreachable;
    }
  }
}
