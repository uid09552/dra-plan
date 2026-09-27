import { Component, ElementRef, signal, viewChild } from '@angular/core';
import { MatDividerModule } from '@angular/material/divider';
import { MatIconModule } from '@angular/material/icon';
import { MatMenuModule, MatMenuTrigger } from '@angular/material/menu';

import { TranslatePipe } from '../i18n/i18n';

export interface MenuItem {
  icon: string;
  /** i18n key. */
  label: string;
  run: () => void;
  danger?: boolean;
  disabled?: boolean;
  /** Draws a divider above the item. */
  divider?: boolean;
}

/**
 * Right-click menu at the pointer position. Open it from `(contextmenu)` handlers or from a
 * "more" button (keyboard users); without pointer coordinates it opens below the target element.
 */
@Component({
  selector: 'app-context-menu',
  imports: [MatMenuModule, MatIconModule, MatDividerModule, TranslatePipe],
  template: `
    <div #anchor class="anchor" [matMenuTriggerFor]="menu" aria-hidden="true"></div>
    <mat-menu #menu="matMenu">
      <ng-template matMenuContent>
        @for (item of items(); track item.label) {
          @if (item.divider) {
            <mat-divider />
          }
          <button
            mat-menu-item
            [class.danger]="item.danger"
            [disabled]="item.disabled"
            (click)="item.run()"
          >
            <mat-icon>{{ item.icon }}</mat-icon>
            <span>{{ item.label | t }}</span>
          </button>
        }
      </ng-template>
    </mat-menu>
  `,
  styles: `
    .anchor {
      position: fixed;
      width: 0;
      height: 0;
    }
    .danger,
    .danger mat-icon {
      color: var(--mat-sys-error);
    }
  `,
})
export class ContextMenu {
  protected readonly items = signal<MenuItem[]>([]);
  private readonly anchor = viewChild.required<ElementRef<HTMLElement>>('anchor');
  private readonly trigger = viewChild.required(MatMenuTrigger);

  open(event: Event, items: MenuItem[]): void {
    event.preventDefault();
    event.stopPropagation();
    let { x, y } = { x: 0, y: 0 };
    if (event instanceof MouseEvent && (event.clientX || event.clientY)) {
      ({ clientX: x, clientY: y } = event);
    } else if (event.target instanceof Element) {
      const rect = event.target.getBoundingClientRect();
      ({ left: x, bottom: y } = rect);
    }
    const style = this.anchor().nativeElement.style;
    style.left = `${x}px`;
    style.top = `${y}px`;
    this.items.set(items);
    const trigger = this.trigger();
    if (trigger.menuOpen) {
      trigger.closeMenu();
    }
    trigger.openMenu();
  }
}
