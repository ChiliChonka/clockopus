import { Injectable, computed, signal } from '@angular/core';

/** `null` stands for “untagged”. */
export type TagKey = string | null;

/**
 * Tasks and tags picked in the day, week and month views to highlight their time. A click picks
 * one (or clears it if it was the only one); Ctrl/⌘+click adds or removes. Shared by the views,
 * so switching from week to day keeps the choice.
 */
@Injectable({ providedIn: 'root' })
export class SelectionService {
  readonly tasks = signal<ReadonlySet<string>>(new Set());
  readonly tags = signal<ReadonlySet<TagKey>>(new Set());

  readonly active = computed(() => this.tasks().size > 0 || this.tags().size > 0);

  toggleTask(id: string, additive: boolean): void {
    this.toggle('task', id, additive);
  }

  toggleTag(tag: TagKey, additive: boolean): void {
    this.toggle('tag', tag, additive);
  }

  clear(): void {
    this.tasks.set(new Set());
    this.tags.set(new Set());
  }

  hasTask(id: string): boolean {
    return this.tasks().has(id);
  }

  hasTag(tag: TagKey): boolean {
    return this.tags().has(tag);
  }

  /** Whether time of a task with these tags is highlighted (everything without a selection). */
  matches(taskId: string, tags: readonly string[]): boolean {
    if (!this.active()) return true;
    if (this.tasks().has(taskId)) return true;
    const picked = this.tags();
    return tags.length === 0 ? picked.has(null) : tags.some((t) => picked.has(t));
  }

  private toggle(kind: 'task' | 'tag', key: string | null, additive: boolean): void {
    const tasks = new Set(this.tasks());
    const tags = new Set(this.tags());
    const own: Set<string | null> = kind === 'task' ? tasks : tags;
    const other = kind === 'task' ? tags : tasks;
    if (additive) {
      if (own.has(key)) own.delete(key);
      else own.add(key);
    } else {
      const onlyThis = own.size === 1 && own.has(key) && other.size === 0;
      tasks.clear();
      tags.clear();
      if (!onlyThis) own.add(key);
    }
    this.tasks.set(tasks);
    this.tags.set(tags);
  }
}
