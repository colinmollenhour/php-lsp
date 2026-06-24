<?php

namespace App;

class Grandchild extends \App\Base
{
    public function launch(): void
    {
        $this->boot();
    }
}
