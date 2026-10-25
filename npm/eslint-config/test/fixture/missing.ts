type T = 'a' | 'b';

export function f(v: T): number {
  switch (v) {
    case 'a':
      return 1;
  }
  return 0;
}
