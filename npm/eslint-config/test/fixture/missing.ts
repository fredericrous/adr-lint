type Shape = 'circle' | 'square';

export function sidesOf(shape: Shape): number {
  switch (shape) {
    case 'circle':
      return 1;
  }
  return 0;
}
