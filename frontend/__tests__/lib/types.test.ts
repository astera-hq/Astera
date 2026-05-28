import { parseStellarAddress, isStellarAddress } from '@/lib/types';

describe('Stellar address validation', () => {
  const validAddress = 'G' + 'A'.repeat(55);

  it('parses a valid Stellar address successfully', () => {
    const parsed = parseStellarAddress(validAddress);
    expect(parsed).toBe(validAddress);
    expect(isStellarAddress(parsed)).toBe(true);
  });

  it('throws when the address is invalid', () => {
    expect(() => parseStellarAddress('invalid')).toThrow('Invalid Stellar address');
    expect(isStellarAddress('invalid')).toBe(false);
  });
});
