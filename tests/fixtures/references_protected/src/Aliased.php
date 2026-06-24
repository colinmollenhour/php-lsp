<?php

namespace App\Sub;

use App\Base as TheBase;

class Aliased extends TheBase
{
    public function fire(): void
    {
        $this->boot();
    }
}
