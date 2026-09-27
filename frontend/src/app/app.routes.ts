import { Routes } from '@angular/router';

import { Shell } from './layout/shell/shell';

export const routes: Routes = [
  {
    path: '',
    component: Shell,
    children: [
      { path: '', pathMatch: 'full', redirectTo: 'dashboard' },
      {
        path: 'dashboard',
        loadComponent: () => import('./features/dashboard/dashboard').then((m) => m.Dashboard),
      },
      {
        path: 'services',
        loadComponent: () =>
          import('./features/services/services-list').then((m) => m.ServicesList),
      },
      {
        path: 'services/:id',
        loadComponent: () =>
          import('./features/services/service-detail').then((m) => m.ServiceDetail),
      },
      {
        path: 'recovery',
        loadComponent: () => import('./features/recovery/recovery').then((m) => m.Recovery),
      },
      {
        path: 'catalog',
        loadComponent: () => import('./features/catalog/catalog').then((m) => m.CatalogPage),
      },
      {
        path: 'settings',
        loadComponent: () => import('./features/settings/settings').then((m) => m.Settings),
      },
      {
        path: 'profile',
        loadComponent: () => import('./features/profile/profile').then((m) => m.Profile),
      },
      { path: '**', redirectTo: 'dashboard' },
    ],
  },
];
