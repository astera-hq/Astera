import { promises as fs } from 'fs';
import { join } from 'path';

const PREFS_DIR = join(process.cwd(), '.data', 'notification-prefs');

export type NotificationPreferences = {
  email?: {
    enabled: boolean;
    email: string;
    events: string[];
  };
  webhook?: {
    enabled: boolean;
    url: string;
    events: string[];
  };
};

async function ensureDir() {
  try {
    await fs.mkdir(PREFS_DIR, { recursive: true });
  } catch {
    // already exists
  }
}

function getFilePath(account: string): string {
  const safe = account.replace(/[^A-Z0-9]/gi, '_');
  return join(PREFS_DIR, `${safe}.json`);
}

export async function savePreferences(account: string, prefs: NotificationPreferences): Promise<void> {
  await ensureDir();
  const path = getFilePath(account);
  await fs.writeFile(path, JSON.stringify(prefs, null, 2), 'utf-8');
}

export async function getPreferences(account: string): Promise<NotificationPreferences | null> {
  try {
    const path = getFilePath(account);
    const content = await fs.readFile(path, 'utf-8');
    return JSON.parse(content) as NotificationPreferences;
  } catch {
    return null;
  }
}

export async function deletePreferences(account: string): Promise<boolean> {
  try {
    const path = getFilePath(account);
    await fs.unlink(path);
    return true;
  } catch {
    return false;
  }
}
