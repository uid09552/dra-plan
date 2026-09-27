import { NgTemplateOutlet } from '@angular/common';
import { Component, computed, inject, input } from '@angular/core';
import { rxResource } from '@angular/core/rxjs-interop';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';
import { MatProgressBarModule } from '@angular/material/progress-bar';

import { Api } from '../../../core/api/api';
import { I18n, TranslatePipe } from '../../../core/i18n/i18n';
import { parseInline, parseMarkdown, Span } from '../../../core/markdown/markdown';

/** Live preview of the emergency handbook generated from the current working data. */
@Component({
  selector: 'app-handbook-preview',
  imports: [NgTemplateOutlet, MatButtonModule, MatIconModule, MatProgressBarModule, TranslatePipe],
  template: `
    <div class="d-flex flex-wrap gap-2 align-items-center mb-3">
      <span class="muted flex-grow-1">{{ 'handbook.hint' | t }}</span>
      <button mat-stroked-button (click)="handbook.reload()">
        <mat-icon>refresh</mat-icon>{{ 'handbook.refresh' | t }}
      </button>
      <button mat-flat-button (click)="download()" [disabled]="!handbook.hasValue()">
        <mat-icon>download</mat-icon>{{ 'handbook.download' | t }}
      </button>
    </div>
    @if (handbook.isLoading()) {
      <mat-progress-bar mode="indeterminate" />
    }
    <article class="handbook">
      @for (block of blocks(); track $index) {
        @switch (block.kind) {
          @case ('heading') {
            <div [class]="'h h' + block.level" role="heading" [attr.aria-level]="block.level">
              <ng-container *ngTemplateOutlet="inline; context: { $implicit: spans(block.text) }" />
            </div>
          }
          @case ('quote') {
            <blockquote>
              <ng-container *ngTemplateOutlet="inline; context: { $implicit: spans(block.text) }" />
            </blockquote>
          }
          @case ('paragraph') {
            <p>
              <ng-container *ngTemplateOutlet="inline; context: { $implicit: spans(block.text) }" />
            </p>
          }
          @case ('list') {
            <ul>
              @for (item of block.items; track $index) {
                <li [class.indent]="item.indent" [class.task]="item.checkbox">
                  @if (item.checkbox) {
                    <mat-icon class="box">check_box_outline_blank</mat-icon>
                  }
                  <span
                    ><ng-container
                      *ngTemplateOutlet="inline; context: { $implicit: spans(item.text) }"
                  /></span>
                </li>
              }
            </ul>
          }
          @case ('table') {
            <div class="table-scroll">
              <table>
                <thead>
                  <tr>
                    @for (h of block.header; track $index) {
                      <th>{{ h }}</th>
                    }
                  </tr>
                </thead>
                <tbody>
                  @for (row of block.rows; track $index) {
                    <tr>
                      @for (c of row; track $index) {
                        <td>{{ c }}</td>
                      }
                    </tr>
                  }
                </tbody>
              </table>
            </div>
          }
        }
      }
    </article>

    <ng-template #inline let-spans>
      @for (s of spans; track $index) {
        @if (s.bold) {
          <strong>{{ s.text }}</strong>
        } @else if (s.italic) {
          <em>{{ s.text }}</em>
        } @else {
          {{ s.text }}
        }
      }
    </ng-template>
  `,
  styles: `
    .handbook {
      max-width: 960px;
      border: 1px solid var(--mat-sys-outline-variant);
      border-radius: 12px;
      padding: 16px 20px;
      background: var(--mat-sys-surface-container-lowest);
    }
    .h {
      margin: 20px 0 8px;
    }
    .h1 {
      font: var(--mat-sys-headline-small);
      margin-top: 0;
    }
    .h2 {
      font: var(--mat-sys-title-large);
      border-bottom: 1px solid var(--mat-sys-outline-variant);
      padding-bottom: 4px;
    }
    .h3 {
      font: var(--mat-sys-title-medium);
    }
    blockquote {
      margin: 8px 0;
      padding: 8px 12px;
      border-left: 4px solid var(--mat-sys-tertiary);
      background: var(--mat-sys-tertiary-container);
      color: var(--mat-sys-on-tertiary-container);
      border-radius: 4px;
    }
    ul {
      list-style: none;
      padding-left: 0;
    }
    li {
      display: flex;
      gap: 6px;
      padding: 2px 0;
    }
    li:not(.task)::before {
      content: '•';
    }
    li.indent {
      padding-left: 30px;
      color: var(--mat-sys-on-surface-variant);
    }
    .box {
      font-size: 20px;
      width: 20px;
      height: 20px;
    }
    table {
      border-collapse: collapse;
      margin: 8px 0 16px;
      min-width: 100%;
    }
    th,
    td {
      border: 1px solid var(--mat-sys-outline-variant);
      padding: 4px 8px;
      text-align: left;
      vertical-align: top;
    }
    th {
      background: var(--mat-sys-surface-container);
    }
  `,
})
export class HandbookPreview {
  readonly serviceId = input.required<string>();
  readonly serviceName = input('service');
  /** Incremented by the parent after changes. */
  readonly reloadKey = input(0);

  private readonly api = inject(Api);
  private readonly i18n = inject(I18n);

  protected readonly handbook = rxResource({
    params: () => ({ id: this.serviceId(), key: this.reloadKey(), lang: this.i18n.lang() }),
    stream: ({ params }) => this.api.draftHandbook(params.id),
  });
  protected readonly blocks = computed(() => parseMarkdown(this.handbook.value() ?? ''));

  protected spans(text: string): Span[] {
    return parseInline(text);
  }

  protected download(): void {
    const text = this.handbook.value();
    if (!text) {
      return;
    }
    const url = URL.createObjectURL(new Blob([text], { type: 'text/markdown' }));
    const a = document.createElement('a');
    a.href = url;
    a.download = `dr-plan-${this.serviceName()}-draft.md`;
    a.click();
    URL.revokeObjectURL(url);
  }
}
