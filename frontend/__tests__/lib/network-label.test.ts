import { Networks } from '@stellar/stellar-sdk';
import { formatNetworkName } from '@/lib/network-label';

describe('formatNetworkName', () => {
  it('maps Stellar network passphrases to friendly labels', () => {
    expect(formatNetworkName(Networks.TESTNET)).toBe('Testnet');
    expect(formatNetworkName(Networks.PUBLIC)).toBe('Mainnet');
    expect(formatNetworkName(Networks.STANDALONE)).toBe('Local');
  });

  it('maps short network identifiers to friendly labels', () => {
    expect(formatNetworkName('TESTNET')).toBe('Testnet');
    expect(formatNetworkName('PUBLIC')).toBe('Mainnet');
    expect(formatNetworkName('standalone')).toBe('Local');
  });

  it('truncates unknown long network values', () => {
    expect(formatNetworkName('Custom Stellar Network ; January 2099')).toBe(
      'Custom Stell...ary 2099',
    );
  });
});
