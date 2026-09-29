import { DOCUMENT } from '@angular/common';
import { Component, inject } from '@angular/core';

/** Redirects the old in-app profile route to the Keycloak account console. */
@Component({
  selector: 'app-profile',
  template: '',
})
export class Profile {
  constructor() {
    inject(DOCUMENT).defaultView?.location.replace('/auth/realms/dra/account');
  }
}
