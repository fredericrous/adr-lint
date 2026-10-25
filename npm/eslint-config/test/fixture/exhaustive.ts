type Shape = 'circle' | 'square';

export function sidesOf(shape: Shape): number {
  switch (shape) {
    case 'circle':
      return 1;
    case 'square':
      return 4;
    default: {
      const unhandled: never = shape;
      return unhandled;
    }
  }
}
